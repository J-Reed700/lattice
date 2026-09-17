#!/usr/bin/env python3
"""Prove that the bundled llama-server sidecar binaries are self-contained.

A sidecar that passes `--help` on the machine that built it can still die on
every user machine: the macOS binary of release llama/b8981 loaded
`@rpath/libllama-common.0.dylib` from the CI build directory and OpenSSL from
Homebrew. This verifier reads the executable formats itself (Mach-O, ELF, PE,
parsed with `struct`), so any host can check every target, and it only
accepts dependencies that every supported user machine provides.

Usage (Python >= 3.9, standard library only, any working directory):

    python3 src-tauri/scripts/verify_llama_binaries.py \
        [--lock PATH] [--require-hashes] [--run] [--expect-all] PATH...

PATH is a binary or a directory; a directory contributes every
`llama-server-*` file inside it. Exit status: 0 when every binary passed (or
was skipped only because this host lacks the Vulkan loader), 1 when any check
failed, 2 for usage or lock-file errors.

Checks, per binary (the target comes from the file name):
  * format and architecture match the target;
  * every dynamic dependency is on the allowlist for that target (macOS:
    /System/Library/ and /usr/lib/ only, no LC_RPATH, minimum macOS <= lock
    `macos_min`, valid code signature; Linux: system glibc libraries only, no
    RPATH/RUNPATH, newest GLIBC_x.y <= lock `glibc_max`; Windows: system DLLs
    only, delay-load imports included);
  * GPU builds link their GPU runtime directly (Metal.framework,
    libvulkan.so.1, vulkan-1.dll), so the backend is compiled in rather than
    loaded from a separate file at runtime;
  * sha256 matches the lock's `sha256` line, when the lock pins hashes;
  * with --run, binaries for this host are copied to an empty directory and
    must answer `--version` under a scrubbed environment.
"""

from __future__ import annotations

import argparse
import dataclasses
import hashlib
import os
import platform
import re
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Callable, Dict, List, Optional, Sequence, Tuple

DEFAULT_LOCK = Path(__file__).resolve().parent / "llama-server.lock"

EXIT_OK = 0
EXIT_FAILED = 1
EXIT_USAGE = 2

RUN_TIMEOUT_SECONDS = 60
RUN_OUTPUT_TAIL_LINES = 20

Version = Tuple[int, ...]


# ---------------------------------------------------------------------------
# Lock file
# ---------------------------------------------------------------------------


class LockError(ValueError):
    """The pin file is missing, unreadable or malformed."""


@dataclasses.dataclass(frozen=True)
class Lock:
    llama_cpp_tag: str
    release: str
    repo: str
    macos_min: str
    glibc_max: str
    # Release file name -> lowercase hex sha256. Empty until the release is
    # published and the lock is updated.
    sha256: Dict[str, str]


LOCK_SCALAR_KEYS = ("llama_cpp_tag", "release", "repo", "macos_min", "glibc_max")
LOCK_VERSION_KEYS = ("macos_min", "glibc_max")
_DOTTED_VERSION_RE = re.compile(r"^\d+(\.\d+){0,2}$")
_SHA256_HEX_RE = re.compile(r"^[0-9a-fA-F]{64}$")


def parse_lock(path) -> Lock:
    """Parse `llama-server.lock`; raise LockError on anything unexpected.

    Format: `key value` lines, `sha256 <hex> <file>` lines, full-line `#`
    comments and blank lines. Every scalar key must appear exactly once.
    """
    path = Path(path)
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise LockError(f"{path}: cannot read lock file: {exc}") from exc

    values: Dict[str, str] = {}
    hashes: Dict[str, str] = {}
    for lineno, raw in enumerate(text.splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        where = f"{path}:{lineno}"
        fields = line.split()
        key = fields[0]
        if key == "sha256":
            if len(fields) != 3:
                raise LockError(f"{where}: expected `sha256 <hex> <file>`, got {raw!r}")
            digest, name = fields[1], fields[2]
            if not _SHA256_HEX_RE.match(digest):
                raise LockError(f"{where}: {digest!r} is not a 64-digit hex sha256")
            if "/" in name or "\\" in name or name in (".", ".."):
                raise LockError(f"{where}: sha256 file must be a bare file name, got {name!r}")
            if name in hashes:
                raise LockError(f"{where}: duplicate sha256 line for {name}")
            hashes[name] = digest.lower()
        elif key in LOCK_SCALAR_KEYS:
            if len(fields) != 2:
                raise LockError(f"{where}: expected `{key} <value>`, got {raw!r}")
            if key in values:
                raise LockError(f"{where}: duplicate key {key!r}")
            values[key] = fields[1]
        else:
            known = ", ".join(LOCK_SCALAR_KEYS + ("sha256",))
            raise LockError(f"{where}: unknown key {key!r} (known keys: {known})")

    missing = [key for key in LOCK_SCALAR_KEYS if key not in values]
    if missing:
        raise LockError(f"{path}: missing required key(s): {', '.join(missing)}")
    for key in LOCK_VERSION_KEYS:
        if not _DOTTED_VERSION_RE.match(values[key]):
            raise LockError(f"{path}: {key} must be a dotted version like 13.3, got {values[key]!r}")
    return Lock(sha256=hashes, **values)


def parse_version(text: str) -> Version:
    return tuple(int(part) for part in text.split("."))


def _pad(version: Version, width: int = 3) -> Version:
    return tuple(version) + (0,) * (width - len(version))


def version_le(a: Version, b: Version) -> bool:
    return _pad(a) <= _pad(b)


def format_version(version: Version) -> str:
    parts = list(version)
    while len(parts) > 2 and parts[-1] == 0:
        parts.pop()
    return ".".join(str(part) for part in parts)


# ---------------------------------------------------------------------------
# Targets and allowlists
# ---------------------------------------------------------------------------


@dataclasses.dataclass(frozen=True)
class Target:
    triple: str
    os: str  # "macos" | "windows" | "linux"
    arch: str  # "aarch64" | "x86_64"
    backend: str  # "metal" | "vulkan" | "cpu"

    @property
    def uses_vulkan(self) -> bool:
        return self.backend == "vulkan"


_MAC = "aarch64-apple-darwin"
_WIN = "x86_64-pc-windows-msvc"
_LINUX = "x86_64-unknown-linux-gnu"

# Every file a release must contain, by exact name. Tauri's externalBin wants
# `<name>-<triple>`; the CPU fallbacks use the `llama-server-cpu-` prefix.
TARGETS: Dict[str, Target] = {
    f"llama-server-{_MAC}": Target(_MAC, "macos", "aarch64", "metal"),
    f"llama-server-{_WIN}.exe": Target(_WIN, "windows", "x86_64", "vulkan"),
    f"llama-server-cpu-{_WIN}.exe": Target(_WIN, "windows", "x86_64", "cpu"),
    f"llama-server-{_LINUX}": Target(_LINUX, "linux", "x86_64", "vulkan"),
    f"llama-server-cpu-{_LINUX}": Target(_LINUX, "linux", "x86_64", "cpu"),
}
EXPECTED_FILES: Tuple[str, ...] = tuple(TARGETS)

# macOS: only libraries that ship inside the OS (sealed system volume and the
# dyld shared cache). Anything else, including @rpath/@loader_path and
# Homebrew or /usr/local paths, is missing on a user's Mac.
MACOS_ALLOWED_DYLIB_PREFIXES = ("/System/Library/", "/usr/lib/")
MACOS_DYLD = "/usr/lib/dyld"
# The Metal build must link Metal itself. Without it the Metal backend was
# left out or lives in a separate library loaded at runtime (GGML_BACKEND_DL).
MACOS_METAL_FRAMEWORK_PREFIX = "/System/Library/Frameworks/Metal.framework/"

# Linux: glibc's own libraries, present on every glibc distribution.
# libstdc++/libgcc_s are linked statically (-static-libstdc++ -static-libgcc)
# and OpenMP is disabled, so libstdc++.so.6, libgcc_s.so.1 and libgomp.so.1
# must not appear.
LINUX_SYSTEM_LIBS = frozenset({
    "libc.so.6",
    "libm.so.6",
    "libpthread.so.0",
    "libdl.so.2",
    "librt.so.1",
    "ld-linux-x86-64.so.2",
})
# The system Vulkan loader; only the Vulkan build may need it. The app falls
# back to the CPU build when it is missing.
LINUX_VULKAN_LIBS = frozenset({"libvulkan.so.1"})
LINUX_INTERPRETER = "/lib64/ld-linux-x86-64.so.2"
# glibc version names that are not GLIBC_x.y, mapped to the glibc release
# that introduced them. An unknown GLIBC_* name fails until it is added here.
GLIBC_ABI_MARKERS: Dict[str, Version] = {
    "GLIBC_ABI_DT_RELR": (2, 36),
}

# Windows: DLLs that ship with every supported Windows 10/11 install. The
# build uses the static MSVC runtime (/MT) and no OpenMP, so VCRUNTIME*,
# MSVCP*, vcomp*, concrt* and libgomp* must not appear. To extend, add the
# DLL with a comment saying why it is guaranteed to exist.
WINDOWS_SYSTEM_DLLS = frozenset(name + ".dll" for name in (
    "kernel32", "advapi32", "ws2_32", "user32", "gdi32", "shell32",
    "ole32", "oleaut32", "bcrypt", "crypt32", "shlwapi", "winmm", "ntdll",
    "dbghelp", "psapi", "iphlpapi", "secur32", "rpcrt4", "version",
    "userenv", "mswsock", "dnsapi", "powrprof", "setupapi", "cfgmgr32",
    "dxgi", "d3d12",
    "ucrtbase",  # the Universal CRT is an OS component since Windows 10
))
# API set contract names, resolved by the loader to OS DLLs.
WINDOWS_API_SET_PREFIXES = ("api-ms-win-", "ext-ms-win-")
# The system Vulkan loader, installed by GPU drivers; Vulkan build only.
WINDOWS_VULKAN_DLLS = frozenset({"vulkan-1.dll"})


# ---------------------------------------------------------------------------
# Binary parsing helpers
# ---------------------------------------------------------------------------


class FormatError(Exception):
    """The bytes are not a well-formed executable of the expected format."""


def _unpack(fmt: str, data: bytes, offset: int) -> tuple:
    size = struct.calcsize(fmt)
    if offset < 0 or offset + size > len(data):
        raise FormatError(
            f"truncated: need {size} bytes at offset {offset:#x}, have {len(data)}"
        )
    return struct.unpack_from(fmt, data, offset)


def _cstring(data: bytes, offset: int, limit: Optional[int] = None) -> str:
    end = len(data) if limit is None else min(limit, len(data))
    if offset < 0 or offset >= end:
        raise FormatError(f"string offset {offset:#x} is out of range")
    nul = data.find(b"\0", offset, end)
    if nul < 0:
        raise FormatError(f"unterminated string at offset {offset:#x}")
    return data[offset:nul].decode("utf-8", errors="replace")


# ---------------------------------------------------------------------------
# Mach-O
# ---------------------------------------------------------------------------

MH_MAGIC = 0xFEEDFACE
MH_CIGAM = 0xCEFAEDFE
MH_MAGIC_64 = 0xFEEDFACF
MH_CIGAM_64 = 0xCFFAEDFE
FAT_MAGIC = 0xCAFEBABE
FAT_MAGIC_64 = 0xCAFEBABF
MH_EXECUTE = 2

CPU_TYPE_X86_64 = 0x01000007
CPU_TYPE_ARM64 = 0x0100000C
CPU_SUBTYPE_ARM64E = 2
CPU_SUBTYPE_MASK = 0xFF000000

LC_REQ_DYLD = 0x80000000
LC_LOAD_DYLIB = 0x0C
LC_LOAD_DYLINKER = 0x0E
LC_LOAD_WEAK_DYLIB = 0x18 | LC_REQ_DYLD
LC_RPATH = 0x1C | LC_REQ_DYLD
LC_CODE_SIGNATURE = 0x1D
LC_REEXPORT_DYLIB = 0x1F | LC_REQ_DYLD
LC_LAZY_LOAD_DYLIB = 0x20
LC_LOAD_UPWARD_DYLIB = 0x23 | LC_REQ_DYLD
LC_VERSION_MIN_MACOSX = 0x24
LC_DYLD_ENVIRONMENT = 0x27
LC_BUILD_VERSION = 0x32

DYLIB_LOAD_COMMANDS = {
    LC_LOAD_DYLIB: "LC_LOAD_DYLIB",
    LC_LOAD_WEAK_DYLIB: "LC_LOAD_WEAK_DYLIB",
    LC_REEXPORT_DYLIB: "LC_REEXPORT_DYLIB",
    LC_LAZY_LOAD_DYLIB: "LC_LAZY_LOAD_DYLIB",
    LC_LOAD_UPWARD_DYLIB: "LC_LOAD_UPWARD_DYLIB",
}

PLATFORM_MACOS = 1

CSMAGIC_EMBEDDED_SIGNATURE = 0xFADE0CC0
CSMAGIC_CODEDIRECTORY = 0xFADE0C02
CSSLOT_CODEDIRECTORY = 0
CSSLOT_ALTERNATE_CODEDIRECTORIES = 0x1000
CS_SUPPORTS_CODELIMIT64 = 0x20300
# CodeDirectory hashType -> (hashlib name, stored hash size).
CS_HASH_TYPES = {1: ("sha1", 20), 2: ("sha256", 32), 3: ("sha256", 20), 4: ("sha384", 48)}


@dataclasses.dataclass
class MachOInfo:
    arch: str
    filetype: int = 0
    dylibs: List[Tuple[str, str]] = dataclasses.field(default_factory=list)
    rpaths: List[str] = dataclasses.field(default_factory=list)
    dyld_environment: List[str] = dataclasses.field(default_factory=list)
    dylinker: Optional[str] = None
    # (platform, minimum OS) per LC_BUILD_VERSION / LC_VERSION_MIN_MACOSX.
    min_versions: List[Tuple[int, Version]] = dataclasses.field(default_factory=list)
    has_code_signature: bool = False
    signature_problem: Optional[str] = None
    universal_slices: List[str] = dataclasses.field(default_factory=list)


def _macho_arch_name(cputype: int, cpusubtype: int) -> str:
    sub = cpusubtype & ~CPU_SUBTYPE_MASK & 0xFFFFFFFF
    if cputype == CPU_TYPE_ARM64:
        return "arm64e" if sub == CPU_SUBTYPE_ARM64E else "arm64"
    if cputype == CPU_TYPE_X86_64:
        return "x86_64"
    return f"cputype {cputype & 0xFFFFFFFF:#x}"


def _decode_macho_version(value: int) -> Version:
    return (value >> 16, (value >> 8) & 0xFF, value & 0xFF)


def parse_macho(data: bytes) -> MachOInfo:
    """Parse a thin arm64/x86_64 Mach-O, or the arm64 slice of a universal one."""
    (magic,) = _unpack(">I", data, 0)
    if magic in (FAT_MAGIC, FAT_MAGIC_64):
        return _parse_universal(data, magic == FAT_MAGIC_64)
    return _parse_thin_macho(data)


def _parse_universal(data: bytes, is64: bool) -> MachOInfo:
    (count,) = _unpack(">I", data, 4)
    if not 0 < count <= 32:
        raise FormatError(f"implausible universal binary slice count {count}")
    slices = []
    offset = 8
    for _ in range(count):
        if is64:
            cputype, cpusubtype, start, size, _align, _reserved = _unpack(">iiQQII", data, offset)
            offset += 32
        else:
            cputype, cpusubtype, start, size, _align = _unpack(">iiIII", data, offset)
            offset += 20
        slices.append((_macho_arch_name(cputype, cpusubtype), start, size))
    names = [name for name, _, _ in slices]
    for name, start, size in slices:
        if name != "arm64":
            continue
        if start + size > len(data):
            raise FormatError("arm64 slice extends past the end of the file")
        info = _parse_thin_macho(data[start:start + size])
        info.universal_slices = names
        return info
    return MachOInfo(arch=f"universal ({', '.join(names)})", universal_slices=names)


def _parse_thin_macho(data: bytes) -> MachOInfo:
    (magic,) = _unpack("<I", data, 0)
    if magic in (MH_MAGIC, MH_CIGAM):
        raise FormatError("32-bit Mach-O")
    if magic == MH_CIGAM_64:
        raise FormatError("big-endian Mach-O")
    if magic != MH_MAGIC_64:
        raise FormatError(f"not a Mach-O file (magic {magic:#010x})")
    _, cputype, cpusubtype, filetype, ncmds, sizeofcmds, _flags, _ = _unpack("<IiiIIIII", data, 0)
    info = MachOInfo(arch=_macho_arch_name(cputype, cpusubtype), filetype=filetype)
    commands_end = 32 + sizeofcmds
    if commands_end > len(data):
        raise FormatError("load commands extend past the end of the file")

    signature = None
    offset = 32
    for index in range(ncmds):
        cmd, cmdsize = _unpack("<II", data, offset)
        if cmdsize < 8 or offset + cmdsize > commands_end:
            raise FormatError(f"load command {index} has invalid size {cmdsize}")
        command = data[offset:offset + cmdsize]
        if cmd in DYLIB_LOAD_COMMANDS:
            (name_offset,) = _unpack("<I", command, 8)
            info.dylibs.append((DYLIB_LOAD_COMMANDS[cmd], _cstring(command, name_offset)))
        elif cmd == LC_RPATH:
            info.rpaths.append(_cstring(command, _unpack("<I", command, 8)[0]))
        elif cmd == LC_DYLD_ENVIRONMENT:
            info.dyld_environment.append(_cstring(command, _unpack("<I", command, 8)[0]))
        elif cmd == LC_LOAD_DYLINKER:
            info.dylinker = _cstring(command, _unpack("<I", command, 8)[0])
        elif cmd == LC_BUILD_VERSION:
            platform_id, minos, _sdk, _ntools = _unpack("<IIII", command, 8)
            info.min_versions.append((platform_id, _decode_macho_version(minos)))
        elif cmd == LC_VERSION_MIN_MACOSX:
            version, _sdk = _unpack("<II", command, 8)
            info.min_versions.append((PLATFORM_MACOS, _decode_macho_version(version)))
        elif cmd == LC_CODE_SIGNATURE:
            signature = _unpack("<II", command, 8)
        offset += cmdsize

    if signature is not None:
        info.has_code_signature = True
        info.signature_problem = verify_code_signature(data, *signature)
    return info


def verify_code_signature(image: bytes, dataoff: int, datasize: int) -> Optional[str]:
    """Check every code page against the CodeDirectory hashes.

    Returns None when the signature covers the whole image and all page
    hashes match, else a description of the problem. The CMS signature is
    not validated: this detects edits made after signing (which make macOS
    kill the process), not who signed.
    """
    if datasize < 12 or dataoff + datasize > len(image):
        return "LC_CODE_SIGNATURE points outside the file"
    blob = image[dataoff:dataoff + datasize]
    try:
        magic, _length, count = _unpack(">III", blob, 0)
        if magic != CSMAGIC_EMBEDDED_SIGNATURE:
            return f"unexpected signature magic {magic:#x}"
        directories = []
        for index in range(count):
            slot, offset = _unpack(">II", blob, 12 + 8 * index)
            if slot == CSSLOT_CODEDIRECTORY or (
                CSSLOT_ALTERNATE_CODEDIRECTORIES <= slot < CSSLOT_ALTERNATE_CODEDIRECTORIES + 5
            ):
                directories.append(offset)
        if not directories:
            return "signature has no CodeDirectory"

        verified = 0
        for cd_offset in directories:
            (cd_magic, _cd_length, version, _flags, hash_offset, _ident, _n_special,
             n_code, code_limit, hash_size, hash_type, _platform, page_log2,
             _spare) = _unpack(">IIIIIIIIIBBBBI", blob, cd_offset)
            if cd_magic != CSMAGIC_CODEDIRECTORY:
                return f"unexpected CodeDirectory magic {cd_magic:#x}"
            if version >= CS_SUPPORTS_CODELIMIT64:
                (code_limit64,) = _unpack(">Q", blob, cd_offset + 56)
                code_limit = code_limit64 or code_limit
            if hash_type not in CS_HASH_TYPES:
                continue
            algorithm, expected_size = CS_HASH_TYPES[hash_type]
            if hash_size != expected_size:
                return f"CodeDirectory hash size {hash_size} does not match hash type {hash_type}"
            if code_limit != dataoff:
                return (f"signature covers {code_limit} bytes but the file has {dataoff} "
                        "bytes before the signature")
            page_size = code_limit if page_log2 == 0 else 1 << page_log2
            if n_code != (code_limit + page_size - 1) // page_size:
                return f"CodeDirectory has {n_code} page hashes for {code_limit} bytes"
            hashes_start = cd_offset + hash_offset
            for page in range(n_code):
                start = page * page_size
                digest = hashlib.new(algorithm, image[start:min(start + page_size, code_limit)])
                want_at = hashes_start + page * hash_size
                if digest.digest()[:hash_size] != blob[want_at:want_at + hash_size]:
                    return (f"code page {page} (offset {start:#x}) does not match its hash; "
                            "the file was modified after signing")
            verified += 1
        if not verified:
            return "no CodeDirectory uses a supported hash type"
    except FormatError as exc:
        return f"malformed signature: {exc}"
    return None


# ---------------------------------------------------------------------------
# ELF
# ---------------------------------------------------------------------------

ELFCLASS64 = 2
ELFDATA2LSB = 1
EM_X86_64 = 62
EM_AARCH64 = 183
PT_LOAD = 1
PT_DYNAMIC = 2
PT_INTERP = 3
PN_XNUM = 0xFFFF
DT_NULL = 0
DT_NEEDED = 1
DT_STRTAB = 5
DT_STRSZ = 10
DT_RPATH = 15
DT_RUNPATH = 29
DT_VERNEED = 0x6FFFFFFE
DT_VERNEEDNUM = 0x6FFFFFFF
_MAX_VERNEED_ENTRIES = 1000


@dataclasses.dataclass
class ElfInfo:
    arch: str
    is_dynamic: bool = False
    interpreter: Optional[str] = None
    needed: List[str] = dataclasses.field(default_factory=list)
    rpath: List[str] = dataclasses.field(default_factory=list)
    runpath: List[str] = dataclasses.field(default_factory=list)
    # (library, version name) from the dynamic version-needs table.
    version_needs: List[Tuple[str, str]] = dataclasses.field(default_factory=list)


def parse_elf(data: bytes) -> ElfInfo:
    """Parse a 64-bit little-endian ELF's program headers and dynamic section."""
    if data[:4] != b"\x7fELF":
        raise FormatError("not an ELF file")
    if len(data) < 16:
        raise FormatError("truncated ELF identification")
    if data[4] != ELFCLASS64:
        return ElfInfo(arch="32-bit ELF")
    if data[5] != ELFDATA2LSB:
        return ElfInfo(arch="big-endian ELF")

    (_type, machine, _version, _entry, phoff, _shoff, _flags, _ehsize,
     phentsize, phnum, _shentsize, _shnum, _shstrndx) = _unpack("<HHIQQQIHHHHHH", data, 16)
    info = ElfInfo(arch={EM_X86_64: "x86_64", EM_AARCH64: "aarch64"}.get(machine, f"e_machine {machine}"))
    if phnum == PN_XNUM:
        raise FormatError("extended program header numbering is not supported")
    if phnum and phentsize != 56:
        raise FormatError(f"unexpected program header size {phentsize}")

    loads = []
    dynamic = None
    for index in range(phnum):
        p_type, _pflags, p_offset, p_vaddr, _paddr, p_filesz, _memsz, _align = _unpack(
            "<IIQQQQQQ", data, phoff + index * 56)
        if p_type == PT_LOAD:
            loads.append((p_vaddr, p_offset, p_filesz))
        elif p_type == PT_INTERP:
            info.interpreter = _cstring(data, p_offset, p_offset + p_filesz)
        elif p_type == PT_DYNAMIC:
            dynamic = (p_offset, p_filesz)
    if dynamic is None:
        return info
    info.is_dynamic = True

    def file_offset(vaddr: int) -> int:
        for seg_vaddr, seg_offset, seg_size in loads:
            if seg_vaddr <= vaddr < seg_vaddr + seg_size:
                return seg_offset + (vaddr - seg_vaddr)
        raise FormatError(f"address {vaddr:#x} is not inside any PT_LOAD segment")

    entries: List[Tuple[int, int]] = []
    dyn_offset, dyn_size = dynamic
    for offset in range(dyn_offset, dyn_offset + dyn_size - 15, 16):
        tag, value = _unpack("<qQ", data, offset)
        if tag == DT_NULL:
            break
        entries.append((tag, value))
    tags: Dict[int, int] = {}
    for tag, value in entries:
        tags.setdefault(tag, value)

    if DT_STRTAB not in tags or DT_STRSZ not in tags:
        if any(tag in (DT_NEEDED, DT_RPATH, DT_RUNPATH, DT_VERNEED) for tag, _ in entries):
            raise FormatError("dynamic section has no string table")
        return info
    strtab = file_offset(tags[DT_STRTAB])
    strsz = tags[DT_STRSZ]

    def dynstr(index: int) -> str:
        if index >= strsz:
            raise FormatError(f"string index {index} is outside the dynamic string table")
        return _cstring(data, strtab + index, strtab + strsz)

    for tag, value in entries:
        if tag == DT_NEEDED:
            info.needed.append(dynstr(value))
        elif tag == DT_RPATH:
            info.rpath.append(dynstr(value))
        elif tag == DT_RUNPATH:
            info.runpath.append(dynstr(value))

    if DT_VERNEED in tags:
        offset = file_offset(tags[DT_VERNEED])
        for _ in range(tags.get(DT_VERNEEDNUM) or _MAX_VERNEED_ENTRIES):
            _vn_version, vn_cnt, vn_file, vn_aux, vn_next = _unpack("<HHIII", data, offset)
            library = dynstr(vn_file)
            aux = offset + vn_aux
            for _ in range(vn_cnt):
                _hash, _vflags, _other, vna_name, vna_next = _unpack("<IHHII", data, aux)
                info.version_needs.append((library, dynstr(vna_name)))
                if vna_next == 0:
                    break
                aux += vna_next
            if vn_next == 0:
                break
            offset += vn_next
    return info


# ---------------------------------------------------------------------------
# PE
# ---------------------------------------------------------------------------

IMAGE_FILE_MACHINE_I386 = 0x014C
IMAGE_FILE_MACHINE_AMD64 = 0x8664
IMAGE_FILE_MACHINE_ARM64 = 0xAA64
IMAGE_FILE_DLL = 0x2000
PE32_MAGIC = 0x10B
PE32_PLUS_MAGIC = 0x20B
IMAGE_DIRECTORY_ENTRY_IMPORT = 1
IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT = 13
_MAX_IMPORT_DESCRIPTORS = 4096


@dataclasses.dataclass
class PeInfo:
    arch: str
    is_dll: bool = False
    imports: List[str] = dataclasses.field(default_factory=list)
    delay_imports: List[str] = dataclasses.field(default_factory=list)


def parse_pe(data: bytes) -> PeInfo:
    """Parse a PE32+ image's import and delay-load import directories."""
    if data[:2] != b"MZ":
        raise FormatError("not a PE file (no MZ header)")
    (pe_offset,) = _unpack("<I", data, 0x3C)
    if data[pe_offset:pe_offset + 4] != b"PE\0\0":
        raise FormatError("missing PE signature")
    machine, nsections, _ts, _symtab, _nsyms, opt_size, characteristics = _unpack(
        "<HHIIIHH", data, pe_offset + 4)
    arch = {
        IMAGE_FILE_MACHINE_AMD64: "x86_64",
        IMAGE_FILE_MACHINE_ARM64: "aarch64",
        IMAGE_FILE_MACHINE_I386: "i386",
    }.get(machine, f"machine {machine:#06x}")
    opt = pe_offset + 24
    (magic,) = _unpack("<H", data, opt)
    if magic == PE32_MAGIC:
        return PeInfo(arch=f"32-bit PE ({arch})")
    if magic != PE32_PLUS_MAGIC:
        raise FormatError(f"unknown optional header magic {magic:#x}")
    info = PeInfo(arch=arch, is_dll=bool(characteristics & IMAGE_FILE_DLL))

    (image_base,) = _unpack("<Q", data, opt + 24)
    (size_of_headers,) = _unpack("<I", data, opt + 60)
    (n_dirs,) = _unpack("<I", data, opt + 108)
    n_dirs = min(n_dirs, 16)
    if 112 + 8 * n_dirs > opt_size:
        raise FormatError("data directories extend past the optional header")
    directories = [_unpack("<II", data, opt + 112 + 8 * i) for i in range(n_dirs)]

    sections = []
    table = opt + opt_size
    for index in range(nsections):
        _name, vsize, vaddr, raw_size, raw_ptr = _unpack("<8sIIII", data, table + 40 * index)
        sections.append((vaddr, max(vsize, raw_size), raw_ptr, raw_size))

    def file_offset(rva: int) -> int:
        for vaddr, span, raw_ptr, raw_size in sections:
            if vaddr <= rva < vaddr + span:
                if rva - vaddr >= raw_size:
                    raise FormatError(f"RVA {rva:#x} has no file data")
                return raw_ptr + (rva - vaddr)
        if rva < size_of_headers:
            return rva
        raise FormatError(f"RVA {rva:#x} is not inside any section")

    def directory(index: int) -> int:
        return directories[index][0] if index < len(directories) else 0

    import_rva = directory(IMAGE_DIRECTORY_ENTRY_IMPORT)
    if import_rva:
        offset = file_offset(import_rva)
        for index in range(_MAX_IMPORT_DESCRIPTORS):
            _ilt, _ts, _fwd, name_rva, _iat = _unpack("<IIIII", data, offset + 20 * index)
            if name_rva == 0:
                break
            info.imports.append(_cstring(data, file_offset(name_rva)))
        else:
            raise FormatError("import directory is not terminated")

    delay_rva = directory(IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT)
    if delay_rva:
        offset = file_offset(delay_rva)
        for index in range(_MAX_IMPORT_DESCRIPTORS):
            attributes, name, *_rest = _unpack("<8I", data, offset + 32 * index)
            if name == 0:
                break
            # Attribute bit 0 set: fields are RVAs. Clear: legacy VAs.
            name_rva = name if attributes & 1 else name - image_base
            info.delay_imports.append(_cstring(data, file_offset(name_rva)))
        else:
            raise FormatError("delay-load import directory is not terminated")
    return info


# ---------------------------------------------------------------------------
# Policy checks
# ---------------------------------------------------------------------------


@dataclasses.dataclass
class Report:
    path: Path
    target: Optional[Target]
    parsed: bool = False
    deps: Optional[List[str]] = None
    facts: List[str] = dataclasses.field(default_factory=list)
    failures: List[str] = dataclasses.field(default_factory=list)
    notes: List[str] = dataclasses.field(default_factory=list)
    # Set when the run check could not execute for a benign host reason.
    skipped: Optional[str] = None

    @property
    def status(self) -> str:
        if self.failures:
            return "FAIL"
        if self.skipped:
            return "SKIP"
        return "PASS"

    def fail(self, message: str) -> None:
        self.failures.append(message)


def _is_system_dylib(path: str) -> bool:
    if not path.startswith(MACOS_ALLOWED_DYLIB_PREFIXES):
        return False
    # Reject `/usr/lib/../local/lib/x.dylib` and similar escapes.
    return all(part not in ("", ".", "..") for part in path[1:].split("/"))


def _short_dylib_name(path: str) -> str:
    match = re.match(r"^/System/Library/(?:Private)?Frameworks/([^/]+)\.framework/", path)
    return match.group(1) if match else path.rsplit("/", 1)[-1]


def check_macos(info: MachOInfo, target: Target, lock: Lock, report: Report) -> None:
    if info.arch != "arm64":
        report.fail(f"architecture is {info.arch}; {target.triple} needs a thin arm64 "
                    "Mach-O (or a universal binary with an arm64 slice)")
        return
    if info.universal_slices:
        report.notes.append(f"universal binary ({', '.join(info.universal_slices)}); "
                            "checked the arm64 slice")
    if info.filetype != MH_EXECUTE:
        report.fail(f"Mach-O file type is {info.filetype}, expected MH_EXECUTE")

    report.deps = [_short_dylib_name(path) for _, path in info.dylibs]
    for command, path in info.dylibs:
        if not _is_system_dylib(path):
            report.fail(f"{command} outside /System/Library and /usr/lib: {path}")
    for rpath in info.rpaths:
        report.fail(f"LC_RPATH present (a self-contained binary needs none): {rpath}")
    for entry in info.dyld_environment:
        report.fail(f"LC_DYLD_ENVIRONMENT present: {entry}")
    if info.dylinker != MACOS_DYLD:
        report.fail(f"dynamic linker is {info.dylinker or 'missing'}, expected {MACOS_DYLD}")
    if target.backend == "metal" and not any(
            path.startswith(MACOS_METAL_FRAMEWORK_PREFIX) for _, path in info.dylibs):
        report.fail("Metal build does not link Metal.framework; the Metal backend is "
                    "missing or loaded at runtime")

    macos_versions = [version for platform_id, version in info.min_versions
                      if platform_id == PLATFORM_MACOS]
    limit = parse_version(lock.macos_min)
    if not macos_versions:
        report.fail("no minimum macOS version (LC_BUILD_VERSION for macOS or LC_VERSION_MIN_MACOSX)")
    else:
        minos = max(macos_versions, key=_pad)
        if version_le(minos, limit):
            report.facts.append(f"min macOS {format_version(minos)} (lock {lock.macos_min})")
        else:
            report.fail(f"minimum macOS {format_version(minos)} is newer than lock "
                        f"macos_min {lock.macos_min}")

    if not info.has_code_signature:
        report.fail("no code signature; arm64 macOS kills unsigned executables "
                    "(the linker's ad-hoc signature is enough)")
    elif info.signature_problem:
        report.fail(f"code signature invalid: {info.signature_problem}")
    else:
        report.facts.append("code pages match signature")


_GLIBC_VERSION_RE = re.compile(r"^GLIBC_(\d+)\.(\d+)(?:\.(\d+))?$")


def check_linux(info: ElfInfo, target: Target, lock: Lock, report: Report) -> None:
    if info.arch != "x86_64":
        report.fail(f"architecture is {info.arch}; {target.triple} needs an x86-64 ELF64")
        return
    report.deps = list(info.needed)
    allowed = LINUX_SYSTEM_LIBS | (LINUX_VULKAN_LIBS if target.uses_vulkan else frozenset())
    for library in info.needed:
        if library in allowed:
            continue
        if library in LINUX_VULKAN_LIBS:
            report.fail(f"DT_NEEDED {library} is only allowed in the Vulkan build, "
                        "not the CPU fallback")
        else:
            report.fail(f"DT_NEEDED not a glibc system library: {library}")
    if target.uses_vulkan and not LINUX_VULKAN_LIBS & set(info.needed):
        report.fail("Vulkan build does not link libvulkan.so.1; the Vulkan backend is "
                    "missing or loaded at runtime")
    for path in info.rpath:
        report.fail(f"DT_RPATH present: {path}")
    for path in info.runpath:
        report.fail(f"DT_RUNPATH present: {path}")
    if info.interpreter is not None and info.interpreter != LINUX_INTERPRETER:
        report.fail(f"program interpreter is {info.interpreter}, expected {LINUX_INTERPRETER}")
    if not info.is_dynamic:
        report.facts.append("statically linked")
        return

    newest: Optional[Tuple[Version, str]] = None
    for library, name in info.version_needs:
        if not name.startswith("GLIBC_"):
            continue  # GLIBCXX_/GCC_ names belong to libraries rejected above.
        match = _GLIBC_VERSION_RE.match(name)
        if match:
            version = tuple(int(part) for part in match.groups() if part is not None)
        elif name in GLIBC_ABI_MARKERS:
            version = GLIBC_ABI_MARKERS[name]
        elif name == "GLIBC_PRIVATE":
            report.fail(f"references GLIBC_PRIVATE symbols from {library}")
            continue
        else:
            report.fail(f"unrecognized glibc version {name} (from {library}); add it to "
                        "GLIBC_ABI_MARKERS with the glibc release that introduced it")
            continue
        if newest is None or _pad(version) > _pad(newest[0]):
            newest = (version, name)
    if newest is None:
        report.notes.append("no GLIBC_* symbol versions referenced")
    elif version_le(newest[0], parse_version(lock.glibc_max)):
        report.facts.append(f"needs {newest[1]} (lock glibc_max {lock.glibc_max})")
    else:
        report.fail(f"needs {newest[1]}, newer than lock glibc_max {lock.glibc_max}")


def is_allowed_windows_dll(name: str, target: Target) -> bool:
    lowered = name.lower()
    if lowered in WINDOWS_SYSTEM_DLLS or lowered.startswith(WINDOWS_API_SET_PREFIXES):
        return True
    return target.uses_vulkan and lowered in WINDOWS_VULKAN_DLLS


def check_windows(info: PeInfo, target: Target, lock: Lock, report: Report) -> None:
    del lock  # Windows has no version pin.
    if info.arch != "x86_64":
        report.fail(f"architecture is {info.arch}; {target.triple} needs a PE32+ x86-64 image")
        return
    if info.is_dll:
        report.fail("image is a DLL, not an executable")
    report.deps = info.imports + [f"{name} (delay-load)" for name in info.delay_imports]
    if target.uses_vulkan and not any(
            name.lower() in WINDOWS_VULKAN_DLLS for name in info.imports + info.delay_imports):
        report.fail("Vulkan build does not import vulkan-1.dll; the Vulkan backend is "
                    "missing or loaded at runtime")
    for kind, names in (("import", info.imports), ("delay-load import", info.delay_imports)):
        for name in names:
            if is_allowed_windows_dll(name, target):
                continue
            if name.lower() in WINDOWS_VULKAN_DLLS:
                report.fail(f"{kind} {name} is only allowed in the Vulkan build, "
                            "not the CPU fallback")
            else:
                report.fail(f"{kind} not a Windows system DLL: {name}")


FORMATS: Dict[str, Tuple[str, Callable[[bytes], object], Callable[..., None]]] = {
    "macos": ("Mach-O", parse_macho, check_macos),
    "linux": ("ELF", parse_elf, check_linux),
    "windows": ("PE", parse_pe, check_windows),
}


def check_hash(name: str, data: bytes, lock: Lock, require_hashes: bool, report: Report) -> None:
    if not lock.sha256:
        if require_hashes:
            report.fail("sha256: the lock pins no hashes (--require-hashes)")
        return
    expected = lock.sha256.get(name)
    if expected is None:
        report.fail(f"sha256: {name} is not pinned in the lock")
        return
    actual = hashlib.sha256(data).hexdigest()
    if actual == expected:
        report.facts.append("sha256 matches lock")
    else:
        report.fail(f"sha256 mismatch: file {actual}, lock {expected}")


# ---------------------------------------------------------------------------
# Runtime check
# ---------------------------------------------------------------------------

WINDOWS_STATUS_DLL_NOT_FOUND = 0xC0000135
_LINUX_MISSING_VULKAN_RE = re.compile(r"libvulkan\.so\.1: cannot open shared object file")
VULKAN_LOADER_MISSING = "Vulkan loader not installed on this host"


@dataclasses.dataclass
class RunResult:
    returncode: Optional[int]  # None when the process did not finish
    output: str
    timed_out: bool = False


def host_platform() -> Tuple[str, str]:
    """(os, arch) of this machine, in Target terms."""
    system = platform.system()
    machine = platform.machine().lower()
    arch = {"arm64": "aarch64", "aarch64": "aarch64", "x86_64": "x86_64",
            "amd64": "x86_64", "x64": "x86_64"}.get(machine, machine)
    # An x86_64 Python under Rosetta still runs on an arm64 kernel.
    if system == "Darwin" and arch == "x86_64" and "ARM64" in platform.uname().version:
        arch = "aarch64"
    os_name = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}.get(system, system.lower())
    return os_name, arch


def scrubbed_env(environ) -> Dict[str, str]:
    """The environment minus loader overrides that could mask missing libraries."""
    return {key: value for key, value in environ.items()
            if not key.upper().startswith(("DYLD_", "LD_"))}


def windows_vulkan_loader_findable(environ, isfile: Callable[[str], bool] = os.path.isfile) -> bool:
    root = environ.get("SystemRoot") or environ.get("SYSTEMROOT") or r"C:\Windows"
    directories = [os.path.join(root, "System32")]
    directories += [entry for entry in environ.get("PATH", "").split(os.pathsep) if entry]
    return any(isfile(os.path.join(directory, name))
               for directory in directories for name in WINDOWS_VULKAN_DLLS)


def is_missing_vulkan_loader(target: Target, returncode: Optional[int], output: str,
                             windows_loader_findable: bool) -> bool:
    """True when a failed run is explained by the host lacking the Vulkan loader.

    Only the Vulkan builds qualify. Linux: the dynamic loader reports that
    libvulkan.so.1 cannot be opened. Windows: the process ends with
    STATUS_DLL_NOT_FOUND and vulkan-1.dll is in neither System32 nor PATH.
    """
    if not target.uses_vulkan or returncode is None or returncode == 0:
        return False
    if target.os == "linux":
        return _LINUX_MISSING_VULKAN_RE.search(output) is not None
    if target.os == "windows":
        return ((returncode & 0xFFFFFFFF) == WINDOWS_STATUS_DLL_NOT_FOUND
                and not windows_loader_findable)
    return False


def run_isolated(path: Path, timeout: float = RUN_TIMEOUT_SECONDS) -> RunResult:
    """Run `<copy of path> --version` from an otherwise empty directory."""
    workdir = tempfile.mkdtemp(prefix="verify-llama-")
    try:
        copy = Path(workdir) / path.name
        shutil.copyfile(path, copy)
        if os.name != "nt":
            copy.chmod(0o755)
        try:
            proc = subprocess.run(
                [str(copy), "--version"],
                cwd=workdir,
                env=scrubbed_env(os.environ),
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                timeout=timeout,
            )
        except subprocess.TimeoutExpired as exc:
            output = (exc.output or b"").decode("utf-8", errors="replace")
            return RunResult(None, output, timed_out=True)
        except OSError as exc:
            return RunResult(None, f"could not start: {exc}")
        return RunResult(proc.returncode, proc.stdout.decode("utf-8", errors="replace"))
    finally:
        shutil.rmtree(workdir, ignore_errors=True)


def describe_returncode(returncode: int) -> str:
    if returncode < 0:
        try:
            name = signal.Signals(-returncode).name
        except ValueError:
            name = "unknown signal"
        return f"killed by signal {-returncode} ({name})"
    if returncode > 255:
        return f"exit code {returncode} ({returncode & 0xFFFFFFFF:#010x})"
    return f"exit code {returncode}"


def check_run(path: Path, target: Target, report: Report, host: Tuple[str, str],
              timeout: float = RUN_TIMEOUT_SECONDS) -> None:
    if (target.os, target.arch) != host:
        report.notes.append(f"run skipped: host is {host[1]} {host[0]}")
        return
    result = run_isolated(path, timeout)
    if result.returncode == 0 and "version:" in result.output:
        line = next(line for line in result.output.splitlines() if "version:" in line)
        report.facts.append(f"runs ({line.strip()})")
        return

    findable = target.os == "windows" and windows_vulkan_loader_findable(os.environ)
    if is_missing_vulkan_loader(target, result.returncode, result.output, findable):
        if report.failures:
            report.notes.append(f"run skipped: {VULKAN_LOADER_MISSING}")
        else:
            report.skipped = f"{VULKAN_LOADER_MISSING}; static checks passed"
        return

    if result.timed_out:
        summary = f"`--version` did not finish within {timeout:g}s"
    elif result.returncode is None:
        summary = result.output
    elif result.returncode == 0:
        summary = "`--version` exited 0 but printed no `version:` line"
    else:
        summary = f"`--version` failed: {describe_returncode(result.returncode)}"
    lines = [f"run: {summary}"]
    tail = [] if result.returncode is None and not result.timed_out else (
        result.output.splitlines()[-RUN_OUTPUT_TAIL_LINES:])
    if tail:
        lines.append(f"last {len(tail)} line(s) of output:")
        lines.extend(f"  | {line}" for line in tail)
    report.fail("\n".join(lines))


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------


def verify_file(path: Path, lock: Lock, *, require_hashes: bool = False, run: bool = False,
                host: Optional[Tuple[str, str]] = None) -> Report:
    path = Path(path)
    target = TARGETS.get(path.name)
    report = Report(path=path, target=target)
    if target is None:
        report.fail(f"unrecognized file name; expected one of: {', '.join(EXPECTED_FILES)}")
        return report
    try:
        data = path.read_bytes()
    except OSError as exc:
        report.fail(f"cannot read: {exc}")
        return report

    format_name, parse, check = FORMATS[target.os]
    try:
        info = parse(data)
    except FormatError as exc:
        report.fail(f"not a valid {format_name} executable: {exc}")
    else:
        report.parsed = True
        check(info, target, lock, report)
    check_hash(path.name, data, lock, require_hashes, report)
    if run and report.parsed:
        check_run(path, target, report, host or host_platform())
    return report


class UsageError(Exception):
    pass


def collect_binaries(paths: Sequence[str]) -> List[Path]:
    files: List[Path] = []
    for raw in paths:
        path = Path(raw)
        if path.is_dir():
            files.extend(sorted(child for child in path.iterdir()
                                if child.name.startswith("llama-server-") and child.is_file()))
        elif path.is_file():
            files.append(path)
        else:
            raise UsageError(f"no such file or directory: {raw}")
    return files


def format_report(report: Report) -> str:
    target = (f"{report.target.triple}, {report.target.backend}"
              if report.target else "unknown target")
    lines = [f"{report.status} {report.path} [{target}]"]
    if report.deps is not None:
        deps = ", ".join(report.deps) if report.deps else "none"
        lines.append(f"    deps ({len(report.deps)}): {deps}")
    if report.facts:
        lines.append("    " + "; ".join(report.facts))
    for failure in report.failures:
        first, *rest = failure.splitlines()
        lines.append(f"    - {first}")
        lines.extend(f"      {line}" for line in rest)
    if report.skipped:
        lines.append(f"    skip: {report.skipped}")
    for note in report.notes:
        lines.append(f"    note: {note}")
    return "\n".join(lines)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Verify that llama-server sidecar binaries are self-contained "
                    "and runnable on users' machines.")
    parser.add_argument("paths", nargs="+", metavar="PATH",
                        help="binary files, or directories holding llama-server-* files")
    parser.add_argument("--lock", type=Path, default=DEFAULT_LOCK,
                        help=f"pin file (default: {DEFAULT_LOCK})")
    parser.add_argument("--require-hashes", action="store_true",
                        help="fail when the lock pins no sha256 hashes")
    parser.add_argument("--run", action="store_true",
                        help="also run `--version` for binaries built for this host")
    parser.add_argument("--expect-all", action="store_true",
                        help=f"fail unless all {len(EXPECTED_FILES)} release files are present")
    return parser


def _say(line: str) -> None:
    print(line, flush=True)


def main(argv: Optional[Sequence[str]] = None) -> int:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(errors="replace")
    args = build_parser().parse_args(argv)
    try:
        lock = parse_lock(args.lock)
        files = collect_binaries(args.paths)
    except (LockError, UsageError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return EXIT_USAGE

    if not lock.sha256 and not args.require_hashes:
        _say(f"note: {args.lock} pins no sha256 hashes; hash check skipped")
    host = host_platform()
    counts = {"PASS": 0, "FAIL": 0, "SKIP": 0}
    for path in files:
        report = verify_file(path, lock, require_hashes=args.require_hashes,
                             run=args.run, host=host)
        counts[report.status] += 1
        _say(format_report(report))

    failed = counts["FAIL"] > 0
    if not files:
        _say(f"FAIL no llama-server-* binaries found in: {', '.join(args.paths)}")
        failed = True
    if args.expect_all:
        present = {path.name for path in files}
        missing = [name for name in EXPECTED_FILES if name not in present]
        if missing:
            _say(f"FAIL missing release file(s): {', '.join(missing)}")
            failed = True
    _say(f"{counts['PASS']} passed, {counts['FAIL']} failed, {counts['SKIP']} skipped")
    return EXIT_FAILED if failed else EXIT_OK


if __name__ == "__main__":
    sys.exit(main())
