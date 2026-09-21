"""Tests for verify_llama_binaries.py (standard library only).

Run from the repository root:

    python3 -m unittest src-tauri/scripts/test_verify_llama_binaries.py -v

The executables are small synthetic byte blobs that contain only the
structures the verifier reads. To also parse real release binaries, point
VERIFY_LLAMA_REAL_BINARIES at one or more directories (os.pathsep-separated).
"""

from __future__ import annotations

import contextlib
import hashlib
import io
import json
import os
import struct
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest import mock

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

import verify_llama_binaries as vb  # noqa: E402

SCRIPT = SCRIPT_DIR / "verify_llama_binaries.py"

MAC = "llama-server-aarch64-apple-darwin"
MAC_INTEL = "llama-server-x86_64-apple-darwin"
WIN = "llama-server-x86_64-pc-windows-msvc.exe"
WIN_CPU = "llama-server-cpu-x86_64-pc-windows-msvc.exe"
LINUX = "llama-server-x86_64-unknown-linux-gnu"
LINUX_CPU = "llama-server-cpu-x86_64-unknown-linux-gnu"

LOCK = vb.Lock(llama_cpp_tag="b1", release="llama/b1-r1", repo="owner/repo",
               macos_min="13.3", glibc_max="2.35", sha256={})

LOCK_TEXT = textwrap.dedent("""\
    # pin file
    llama_cpp_tag b1

    release llama/b1-r1
    repo owner/repo
    macos_min 13.3
    glibc_max 2.35
""")


def _align(value: int, alignment: int) -> int:
    return (value + alignment - 1) // alignment * alignment


# ---------------------------------------------------------------------------
# Mach-O builder
# ---------------------------------------------------------------------------

SYSTEM_DYLIBS = (
    "/System/Library/Frameworks/Accelerate.framework/Versions/A/Accelerate",
    "/System/Library/Frameworks/Metal.framework/Versions/A/Metal",
    "/usr/lib/libc++.1.dylib",
    "/usr/lib/libSystem.B.dylib",
)


def _lc_dylib(cmd: int, path: str) -> bytes:
    name = path.encode() + b"\0"
    size = _align(24 + len(name), 8)
    return (struct.pack("<IIIIII", cmd, size, 24, 2, 0x10000, 0x10000) + name).ljust(size, b"\0")


def _lc_str(cmd: int, text: str) -> bytes:
    value = text.encode() + b"\0"
    size = _align(12 + len(value), 8)
    return (struct.pack("<III", cmd, size, 12) + value).ljust(size, b"\0")


def _macho_version(version) -> int:
    major, minor, patch = (tuple(version) + (0, 0))[:3]
    return major << 16 | minor << 8 | patch


def build_macho(*, dylibs=SYSTEM_DYLIBS, extra_dylibs=(), rpaths=(), minos=(13, 3),
                platform_id=vb.PLATFORM_MACOS, version_min=False,
                cputype=vb.CPU_TYPE_ARM64, cpusubtype=0, filetype=vb.MH_EXECUTE,
                dylinker="/usr/lib/dyld", dyld_env=(), sign=True) -> bytes:
    cmds = []
    if dylinker:
        cmds.append(_lc_str(vb.LC_LOAD_DYLINKER, dylinker))
    if minos is not None:
        version = _macho_version(minos)
        if version_min:
            cmds.append(struct.pack("<IIII", vb.LC_VERSION_MIN_MACOSX, 16, version, version))
        else:
            cmds.append(struct.pack("<IIIIII", vb.LC_BUILD_VERSION, 24, platform_id,
                                    version, version, 0))
    cmds += [_lc_dylib(vb.LC_LOAD_DYLIB, path) for path in dylibs]
    cmds += [_lc_dylib(cmd, path) for cmd, path in extra_dylibs]
    cmds += [_lc_str(vb.LC_RPATH, path) for path in rpaths]
    cmds += [_lc_str(vb.LC_DYLD_ENVIRONMENT, entry) for entry in dyld_env]
    code = b"\xcc" * 5000  # spans two 4 KiB pages

    sizeofcmds = sum(map(len, cmds)) + (16 if sign else 0)
    header = struct.pack("<IIIIIIII", vb.MH_MAGIC_64, cputype, cpusubtype, filetype,
                         len(cmds) + (1 if sign else 0), sizeofcmds, 0, 0)
    body_len = _align(32 + sizeofcmds + len(code), 16)
    if not sign:
        return (header + b"".join(cmds) + code).ljust(body_len, b"\0")

    page_log2, page = 12, 4096
    slots = (body_len + page - 1) // page
    ident = b"test\0"
    hash_offset = 44 + len(ident)
    cd_len = hash_offset + 32 * slots
    sig_len = 12 + 8 + cd_len
    sig_cmd = struct.pack("<IIII", vb.LC_CODE_SIGNATURE, 16, body_len, sig_len)
    body = (header + b"".join(cmds) + sig_cmd + code).ljust(body_len, b"\0")
    hashes = b"".join(hashlib.sha256(body[i * page:(i + 1) * page]).digest() for i in range(slots))
    code_directory = struct.pack(
        ">IIIIIIIIIBBBBI", vb.CSMAGIC_CODEDIRECTORY, cd_len, 0x20001, 0x20002, hash_offset,
        44, 0, slots, body_len, 32, 2, 0, page_log2, 0) + ident + hashes
    superblob = (struct.pack(">III", vb.CSMAGIC_EMBEDDED_SIGNATURE, sig_len, 1)
                 + struct.pack(">II", vb.CSSLOT_CODEDIRECTORY, 20) + code_directory)
    return body + superblob


def build_universal(slices) -> bytes:
    """slices: [(cputype, cpusubtype, thin Mach-O bytes)]."""
    table = struct.pack(">II", vb.FAT_MAGIC, len(slices))
    offset = _align(8 + 20 * len(slices), 4096)
    payload = b""
    for cputype, cpusubtype, blob in slices:
        table += struct.pack(">iiIII", cputype, cpusubtype, offset + len(payload), len(blob), 12)
        payload += blob.ljust(_align(len(blob), 4096), b"\0")
    return table.ljust(offset, b"\0") + payload


# ---------------------------------------------------------------------------
# ELF builder
# ---------------------------------------------------------------------------

GLIBC_OK = (("libc.so.6", "GLIBC_2.2.5"), ("libc.so.6", "GLIBC_2.34"), ("libm.so.6", "GLIBC_2.29"))


def build_elf(*, needed=("libm.so.6", "libc.so.6"), versions=GLIBC_OK, rpath=None, runpath=None,
              interp=vb.LINUX_INTERPRETER, machine=vb.EM_X86_64, dynamic=True,
              elf_type=vb.ET_DYN, entry=0x1000) -> bytes:
    base = 0x400000
    strtab = bytearray(b"\0")

    def add(text: str) -> int:
        offset = len(strtab)
        strtab.extend(text.encode() + b"\0")
        return offset

    dyn = [(vb.DT_NEEDED, add(name)) for name in needed]
    if rpath is not None:
        dyn.append((vb.DT_RPATH, add(rpath)))
    if runpath is not None:
        dyn.append((vb.DT_RUNPATH, add(runpath)))
    groups = {}
    for library, name in versions:
        groups.setdefault(library, []).append(name)
    library_index = {library: add(library) for library in groups}
    name_index = {(library, name): add(name) for library, name in versions}

    verneed = bytearray()
    libraries = list(groups)
    for i, library in enumerate(libraries):
        names = groups[library]
        vn_next = 16 + 16 * len(names) if i < len(libraries) - 1 else 0
        verneed += struct.pack("<HHIII", 1, len(names), library_index[library], 16, vn_next)
        for j, name in enumerate(names):
            vna_next = 16 if j < len(names) - 1 else 0
            verneed += struct.pack("<IHHII", 0, 0, j + 2, name_index[(library, name)], vna_next)

    phnum = 1 + (1 if interp else 0) + (1 if dynamic else 0)
    cursor = 64 + 56 * phnum
    interp_bytes = interp.encode() + b"\0" if interp else b""
    interp_off = cursor
    cursor += len(interp_bytes)
    strtab_off = cursor
    cursor += len(strtab)
    verneed_off = cursor
    cursor += len(verneed)
    dyn += [(vb.DT_STRTAB, base + strtab_off), (vb.DT_STRSZ, len(strtab))]
    if verneed:
        dyn += [(vb.DT_VERNEED, base + verneed_off), (vb.DT_VERNEEDNUM, len(libraries))]
    dyn.append((vb.DT_NULL, 0))
    dyn_bytes = b"".join(struct.pack("<qQ", tag, value) for tag, value in dyn)
    dyn_off = cursor
    total = cursor + len(dyn_bytes)

    def phdr(p_type, offset, size):
        return struct.pack("<IIQQQQQQ", p_type, 4, offset, base + offset, base + offset,
                           size, size, 1)

    phdrs = phdr(vb.PT_LOAD, 0, total)
    if interp:
        phdrs += phdr(vb.PT_INTERP, interp_off, len(interp_bytes))
    if dynamic:
        phdrs += phdr(vb.PT_DYNAMIC, dyn_off, len(dyn_bytes))
    ident = b"\x7fELF" + bytes([vb.ELFCLASS64, vb.ELFDATA2LSB, 1, 3]) + b"\0" * 8
    ehdr = ident + struct.pack("<HHIQQQIHHHHHH", elf_type, machine, 1, entry, 64, 0, 0, 64, 56,
                               phnum, 64, 0, 0)
    out = ehdr + phdrs + interp_bytes + bytes(strtab) + bytes(verneed)
    if dynamic:
        out += dyn_bytes
    return out


# ---------------------------------------------------------------------------
# PE builder
# ---------------------------------------------------------------------------

WIN_SYSTEM_IMPORTS = ("KERNEL32.dll", "ADVAPI32.dll", "WS2_32.dll",
                      "api-ms-win-core-synch-l1-2-0.dll")


def build_pe(*, imports=WIN_SYSTEM_IMPORTS, delay_imports=(), delay_va_form=False,
             image_base=0x140000000, machine=vb.IMAGE_FILE_MACHINE_AMD64,
             magic=vb.PE32_PLUS_MAGIC, characteristics=0x0022, entry_point=0x1000) -> bytes:
    section_rva, section_raw = 0x1000, 0x400
    imports, delay_imports = tuple(imports), tuple(delay_imports)
    import_len = 20 * (len(imports) + 1) if imports else 0
    delay_len = 32 * (len(delay_imports) + 1) if delay_imports else 0
    names = bytearray()
    name_rvas = []
    for name in imports + delay_imports:
        name_rvas.append(section_rva + import_len + delay_len + len(names))
        names += name.encode() + b"\0"

    content = bytearray()
    for i in range(len(imports)):
        content += struct.pack("<IIIII", 0, 0, 0, name_rvas[i], 0x2000)
    if imports:
        content += b"\0" * 20
    for j in range(len(delay_imports)):
        rva = name_rvas[len(imports) + j]
        if delay_va_form:
            content += struct.pack("<8I", 0, image_base + rva, 0, 0, 0, 0, 0, 0)
        else:
            content += struct.pack("<8I", 1, rva, 0, 0, 0, 0, 0, 0)
    if delay_imports:
        content += b"\0" * 32
    content += names
    raw_size = _align(len(content), 0x200)

    opt = bytearray(240)
    struct.pack_into("<H", opt, 0, magic)
    struct.pack_into("<I", opt, 16, entry_point)
    struct.pack_into("<Q", opt, 24, image_base)
    struct.pack_into("<II", opt, 32, 0x1000, 0x200)
    struct.pack_into("<II", opt, 56, 0x3000, section_raw)
    struct.pack_into("<H", opt, 68, 3)
    struct.pack_into("<I", opt, 108, 16)
    if imports:
        struct.pack_into("<II", opt, 112 + 8 * vb.IMAGE_DIRECTORY_ENTRY_IMPORT,
                         section_rva, import_len)
    if delay_imports:
        struct.pack_into("<II", opt, 112 + 8 * vb.IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT,
                         section_rva + import_len, delay_len)
    coff = struct.pack("<HHIIIHH", machine, 1, 0, 0, 0, len(opt), characteristics)
    section = struct.pack("<8sIIIIIIHHI", b".idata", len(content), section_rva, raw_size,
                          section_raw, 0, 0, 0, 0, 0x40000040)
    dos = bytearray(0x40)
    dos[0:2] = b"MZ"
    struct.pack_into("<I", dos, 0x3C, 0x40)
    headers = bytes(dos) + b"PE\0\0" + coff + bytes(opt) + section
    return headers.ljust(section_raw, b"\0") + bytes(content).ljust(raw_size, b"\0")


# Good synthetic binaries for every release file.
GOOD_BLOBS = {
    MAC: lambda: build_macho(),
    MAC_INTEL: lambda: build_macho(cputype=vb.CPU_TYPE_X86_64,
                                  dylibs=tuple(p for p in SYSTEM_DYLIBS if "Metal.framework" not in p)),
    WIN: lambda: build_pe(imports=WIN_SYSTEM_IMPORTS + ("vulkan-1.dll",)),
    WIN_CPU: lambda: build_pe(),
    LINUX: lambda: build_elf(needed=("libvulkan.so.1", "libm.so.6", "libc.so.6")),
    LINUX_CPU: lambda: build_elf(),
}


class TempDirTestCase(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()

    def write(self, name: str, blob: bytes, directory: Path = None, mode: int = 0o755) -> Path:
        path = (directory or self.dir) / name
        path.write_bytes(blob)
        path.chmod(mode)  # a release file is executable; the verifier checks it
        return path

    def verify(self, name: str, blob: bytes, lock=LOCK, **kwargs) -> vb.Report:
        return vb.verify_file(self.write(name, blob), lock, **kwargs)

    def assertPasses(self, report: vb.Report):
        self.assertEqual(report.status, "PASS", vb.format_report(report))

    def assertFailsWith(self, report: vb.Report, *fragments: str):
        text = vb.format_report(report)
        self.assertEqual(report.status, "FAIL", text)
        for fragment in fragments:
            self.assertTrue(any(fragment in failure for failure in report.failures),
                            f"{fragment!r} not in failures:\n{text}")


class MachOTests(TempDirTestCase):
    def test_intel_cpu_binary_needs_no_metal(self):
        deps = tuple(path for path in SYSTEM_DYLIBS if "Metal.framework" not in path)
        self.assertPasses(self.verify(MAC_INTEL, build_macho(
            cputype=vb.CPU_TYPE_X86_64, dylibs=deps)))

    def test_intel_rejects_arm_binary(self):
        self.assertFailsWith(self.verify(MAC_INTEL, build_macho()), "needs a thin x86_64")

    def test_intel_cpu_rejects_metal_build(self):
        self.assertFailsWith(self.verify(MAC_INTEL, build_macho(
            cputype=vb.CPU_TYPE_X86_64)), "CPU build unexpectedly links Metal.framework")

    def test_intel_checks_its_own_universal_slice(self):
        blob = build_universal([
            (vb.CPU_TYPE_X86_64, 3, build_macho(cputype=vb.CPU_TYPE_X86_64, minos=(14, 0))),
            (vb.CPU_TYPE_ARM64, 0, build_macho()),
        ])
        self.assertPasses(self.verify(MAC, blob))
        self.assertFailsWith(self.verify(MAC_INTEL, blob), "minimum macOS 14.0")

    def test_intel_rejects_unsigned_binary(self):
        self.assertFailsWith(self.verify(MAC_INTEL, build_macho(
            cputype=vb.CPU_TYPE_X86_64, sign=False)), "no code signature")

    def test_system_dependencies_pass(self):
        report = self.verify(MAC, build_macho())
        self.assertPasses(report)
        self.assertEqual(report.deps, ["Accelerate", "Metal", "libc++.1.dylib", "libSystem.B.dylib"])
        self.assertIn("min macOS 13.3 (lock 13.3)", report.facts)

    def test_rpath_dependency_fails(self):
        report = self.verify(MAC, build_macho(
            dylibs=SYSTEM_DYLIBS + ("@rpath/libllama-common.0.dylib",)))
        self.assertFailsWith(report, "LC_LOAD_DYLIB outside /System/Library and /usr/lib: "
                                     "@rpath/libllama-common.0.dylib")

    def test_non_system_absolute_paths_fail(self):
        for path in ("/opt/homebrew/opt/openssl@3/lib/libssl.3.dylib", "/usr/local/lib/libx.dylib",
                     "/Users/runner/work/libggml.dylib", "@executable_path/libggml.dylib",
                     "/usr/lib/../local/lib/libx.dylib", "/usr/libx/liby.dylib"):
            with self.subTest(path=path):
                report = self.verify(MAC, build_macho(dylibs=SYSTEM_DYLIBS + (path,)))
                self.assertFailsWith(report, path)

    def test_every_dylib_load_command_is_checked(self):
        for cmd, label in vb.DYLIB_LOAD_COMMANDS.items():
            with self.subTest(command=label):
                report = self.verify(MAC, build_macho(
                    extra_dylibs=((cmd, "@loader_path/libggml.dylib"),)))
                self.assertFailsWith(report, f"{label} outside /System/Library and /usr/lib: "
                                             "@loader_path/libggml.dylib")

    def test_lc_rpath_fails(self):
        report = self.verify(MAC, build_macho(rpaths=("/usr/lib",)))
        self.assertFailsWith(report, "LC_RPATH present")

    def test_dyld_environment_fails(self):
        report = self.verify(MAC, build_macho(dyld_env=("DYLD_LIBRARY_PATH=@executable_path",)))
        self.assertFailsWith(report, "LC_DYLD_ENVIRONMENT present")

    def test_minos_too_new_fails(self):
        report = self.verify(MAC, build_macho(minos=(14, 0)))
        self.assertFailsWith(report, "minimum macOS 14.0 is newer than lock macos_min 13.3")

    def test_minos_patch_level_above_lock_fails(self):
        report = self.verify(MAC, build_macho(minos=(13, 3, 1)))
        self.assertFailsWith(report, "minimum macOS 13.3.1 is newer")

    def test_version_min_macosx_is_used(self):
        self.assertPasses(self.verify(MAC, build_macho(minos=(12, 0), version_min=True)))
        report = self.verify(MAC, build_macho(minos=(15, 0), version_min=True))
        self.assertFailsWith(report, "minimum macOS 15.0")

    def test_missing_macos_minimum_fails(self):
        self.assertFailsWith(self.verify(MAC, build_macho(minos=None)), "no minimum macOS version")
        ios_only = build_macho(platform_id=2)
        self.assertFailsWith(self.verify(MAC, ios_only), "no minimum macOS version")

    def test_wrong_architecture_fails(self):
        report = self.verify(MAC, build_macho(cputype=vb.CPU_TYPE_X86_64, cpusubtype=3))
        self.assertFailsWith(report, "architecture is x86_64")

    def test_arm64e_fails(self):
        report = self.verify(MAC, build_macho(cpusubtype=vb.CPU_SUBTYPE_ARM64E | 0x80000000))
        self.assertFailsWith(report, "architecture is arm64e")

    def test_non_executable_file_type_fails(self):
        self.assertFailsWith(self.verify(MAC, build_macho(filetype=6)), "expected MH_EXECUTE")

    def test_non_dyld_linker_fails(self):
        report = self.verify(MAC, build_macho(dylinker="/tmp/dyld"))
        self.assertFailsWith(report, "dynamic linker is /tmp/dyld")

    def test_metal_build_must_link_metal(self):
        report = self.verify(MAC, build_macho(dylibs=("/usr/lib/libSystem.B.dylib",)))
        self.assertFailsWith(report, "does not link Metal.framework")

    def test_unsigned_fails(self):
        self.assertFailsWith(self.verify(MAC, build_macho(sign=False)), "no code signature")

    def test_modified_after_signing_fails(self):
        blob = bytearray(build_macho())
        blob[blob.index(b"\xcc" * 16) + 4096] = 0x90  # second code page
        report = self.verify(MAC, bytes(blob))
        self.assertFailsWith(report, "code page 1 (offset 0x1000) does not match its hash")

    def test_universal_binary_checks_arm64_slice(self):
        good = build_universal([(vb.CPU_TYPE_X86_64, 3, build_macho(cputype=vb.CPU_TYPE_X86_64)),
                                (vb.CPU_TYPE_ARM64, 0, build_macho())])
        report = self.verify(MAC, good)
        self.assertPasses(report)
        self.assertIn("universal binary (x86_64, arm64); checked the arm64 slice", report.notes)

        bad = build_universal([(vb.CPU_TYPE_X86_64, 3, build_macho(cputype=vb.CPU_TYPE_X86_64)),
                               (vb.CPU_TYPE_ARM64, 0, build_macho(rpaths=("@loader_path",)))])
        self.assertFailsWith(self.verify(MAC, bad), "LC_RPATH present")

    def test_universal_binary_without_arm64_fails(self):
        blob = build_universal([(vb.CPU_TYPE_X86_64, 3, build_macho(cputype=vb.CPU_TYPE_X86_64))])
        self.assertFailsWith(self.verify(MAC, blob), "architecture is universal (x86_64)")

    def test_other_formats_and_truncation_fail_cleanly(self):
        for label, blob in (("elf", build_elf()), ("pe", build_pe()), ("empty", b""),
                            ("truncated", build_macho()[:100]), ("32-bit", struct.pack("<I", vb.MH_MAGIC))):
            with self.subTest(blob=label):
                self.assertFailsWith(self.verify(MAC, blob), "not a valid Mach-O executable")


class ElfTests(TempDirTestCase):
    def test_glibc_only_passes(self):
        report = self.verify(LINUX_CPU, build_elf())
        self.assertPasses(report)
        self.assertEqual(report.deps, ["libm.so.6", "libc.so.6"])
        self.assertIn("needs GLIBC_2.34 (lock glibc_max 2.35)", report.facts)

    def test_all_glibc_libraries_allowed(self):
        needed = tuple(sorted(vb.LINUX_SYSTEM_LIBS))
        self.assertPasses(self.verify(LINUX_CPU, build_elf(needed=needed)))

    def test_libstdcxx_libgcc_and_libgomp_fail(self):
        for library in ("libstdc++.so.6", "libgcc_s.so.1", "libgomp.so.1", "libllama.so.0"):
            with self.subTest(library=library):
                report = self.verify(LINUX_CPU, build_elf(needed=("libc.so.6", library)))
                self.assertFailsWith(report, f"DT_NEEDED not a glibc system library: {library}")

    def test_vulkan_loader_only_for_vulkan_build(self):
        needed = ("libvulkan.so.1", "libc.so.6")
        self.assertPasses(self.verify(LINUX, build_elf(needed=needed)))
        report = self.verify(LINUX_CPU, build_elf(needed=needed))
        self.assertFailsWith(report, "libvulkan.so.1 is only allowed in the Vulkan build")

    def test_vulkan_build_must_link_vulkan_loader(self):
        report = self.verify(LINUX, build_elf())
        self.assertFailsWith(report, "does not link libvulkan.so.1")

    def test_glibc_too_new_fails(self):
        versions = GLIBC_OK + (("libm.so.6", "GLIBC_2.38"),)
        report = self.verify(LINUX_CPU, build_elf(versions=versions))
        self.assertFailsWith(report, "needs GLIBC_2.38, newer than lock glibc_max 2.35")

    def test_glibc_at_limit_passes(self):
        versions = (("libc.so.6", "GLIBC_2.35"), ("libc.so.6", "GLIBC_2.3.4"))
        report = self.verify(LINUX_CPU, build_elf(versions=versions))
        self.assertPasses(report)
        self.assertIn("needs GLIBC_2.35 (lock glibc_max 2.35)", report.facts)

    def test_glibc_abi_marker_counts_as_its_release(self):
        versions = GLIBC_OK + (("libc.so.6", "GLIBC_ABI_DT_RELR"),)
        report = self.verify(LINUX_CPU, build_elf(versions=versions))
        self.assertFailsWith(report, "needs GLIBC_ABI_DT_RELR, newer than lock glibc_max 2.35")

    def test_unknown_glibc_marker_and_private_fail(self):
        report = self.verify(LINUX_CPU, build_elf(versions=(
            ("libc.so.6", "GLIBC_ABI_SOMETHING"), ("libc.so.6", "GLIBC_PRIVATE"))))
        self.assertFailsWith(report, "unrecognized glibc version GLIBC_ABI_SOMETHING",
                             "references GLIBC_PRIVATE")

    def test_non_glibc_version_names_are_ignored(self):
        # GLIBCXX_ belongs to libstdc++, which the DT_NEEDED check rejects on its own.
        versions = GLIBC_OK + (("libfoo.so.1", "GLIBCXX_3.4.32"),)
        self.assertPasses(self.verify(LINUX_CPU, build_elf(versions=versions)))

    def test_rpath_and_runpath_fail(self):
        report = self.verify(LINUX_CPU, build_elf(rpath="$ORIGIN", runpath="/home/runner/build:"))
        self.assertFailsWith(report, "DT_RPATH present: $ORIGIN",
                             "DT_RUNPATH present: /home/runner/build:")

    def test_nonstandard_interpreter_fails(self):
        report = self.verify(LINUX_CPU, build_elf(interp="/nix/store/x/ld-linux-x86-64.so.2"))
        self.assertFailsWith(report, "program interpreter is /nix/store/x/ld-linux-x86-64.so.2")

    def test_static_binary_passes(self):
        report = self.verify(LINUX_CPU, build_elf(needed=(), versions=(), interp=None, dynamic=False))
        self.assertPasses(report)
        self.assertEqual(report.deps, [])
        self.assertIn("statically linked", report.facts)

    def test_wrong_architecture_fails(self):
        report = self.verify(LINUX_CPU, build_elf(machine=vb.EM_AARCH64))
        self.assertFailsWith(report, "architecture is aarch64")
        report = self.verify(LINUX_CPU, b"\x7fELF\x01\x01\x01\x00" + b"\0" * 44)
        self.assertFailsWith(report, "architecture is 32-bit ELF")

    def test_other_formats_and_truncation_fail_cleanly(self):
        for label, blob in (("macho", build_macho()), ("empty", b""),
                            ("truncated", build_elf()[:120])):
            with self.subTest(blob=label):
                self.assertFailsWith(self.verify(LINUX_CPU, blob), "not a valid ELF executable")

    def test_only_a_program_passes(self):
        # A relocatable object or a core dump parses as ELF64 x86-64 and has
        # nothing else to check, so the type is the only thing that rejects it.
        for elf_type in (0, vb.ET_REL, 4):
            with self.subTest(elf_type=elf_type):
                report = self.verify(LINUX_CPU, build_elf(elf_type=elf_type))
                self.assertFailsWith(report, "expected ET_EXEC or ET_DYN")
        bare = (b"\x7fELF" + bytes([vb.ELFCLASS64, vb.ELFDATA2LSB, 1, 0]) + b"\0" * 8
                + struct.pack("<HH", vb.ET_REL, vb.EM_X86_64) + b"\0" * 44)
        self.assertEqual(len(bare), 64)
        self.assertFailsWith(self.verify(LINUX_CPU, bare), "ELF type is ET_REL")
        self.assertFailsWith(self.verify(LINUX_CPU, build_elf(entry=0)),
                             "ELF entry point is 0")


class PeTests(TempDirTestCase):
    def test_system_dlls_pass_case_insensitively(self):
        imports = ("KERNEL32.dll", "Ws2_32.DLL", "bcrypt.dll", "UCRTBASE.dll",
                   "API-MS-WIN-CRT-RUNTIME-L1-1-0.dll", "ext-ms-win-ntuser-window-l1-1-0.dll")
        report = self.verify(WIN_CPU, build_pe(imports=imports))
        self.assertPasses(report)
        self.assertEqual(report.deps, list(imports))

    def test_msvc_runtime_and_openmp_fail(self):
        for dll in ("VCRUNTIME140.dll", "VCRUNTIME140_1.dll", "MSVCP140.dll", "VCOMP140.DLL",
                    "libgomp-1.dll", "concrt140.dll", "llama.dll", "msvcrt.dll"):
            with self.subTest(dll=dll):
                report = self.verify(WIN_CPU, build_pe(imports=WIN_SYSTEM_IMPORTS + (dll,)))
                self.assertFailsWith(report, f"import not a Windows system DLL: {dll}")

    def test_delay_load_imports_are_checked(self):
        report = self.verify(WIN_CPU, build_pe(delay_imports=("dbghelp.dll", "vcomp140.dll")))
        self.assertFailsWith(report, "delay-load import not a Windows system DLL: vcomp140.dll")
        self.assertEqual(report.deps[-2:], ["dbghelp.dll (delay-load)", "vcomp140.dll (delay-load)"])

    def test_legacy_va_delay_load_descriptor(self):
        report = self.verify(WIN_CPU, build_pe(delay_imports=("MSVCP140.dll",),
                                               delay_va_form=True, image_base=0x10000000))
        self.assertFailsWith(report, "delay-load import not a Windows system DLL: MSVCP140.dll")

    def test_vulkan_loader_only_for_vulkan_build(self):
        with_vulkan = build_pe(imports=WIN_SYSTEM_IMPORTS + ("VULKAN-1.dll",))
        self.assertPasses(self.verify(WIN, with_vulkan))
        self.assertFailsWith(self.verify(WIN_CPU, with_vulkan),
                             "import VULKAN-1.dll is only allowed in the Vulkan build")
        delayed = build_pe(delay_imports=("vulkan-1.dll",))
        self.assertPasses(self.verify(WIN, delayed))
        self.assertFailsWith(self.verify(WIN_CPU, delayed),
                             "delay-load import vulkan-1.dll is only allowed in the Vulkan build")

    def test_vulkan_build_must_import_vulkan_loader(self):
        self.assertFailsWith(self.verify(WIN, build_pe()), "does not import vulkan-1.dll")

    def test_wrong_architecture_fails(self):
        report = self.verify(WIN_CPU, build_pe(machine=vb.IMAGE_FILE_MACHINE_ARM64))
        self.assertFailsWith(report, "architecture is aarch64")
        report = self.verify(WIN_CPU, build_pe(machine=vb.IMAGE_FILE_MACHINE_I386, magic=vb.PE32_MAGIC))
        self.assertFailsWith(report, "architecture is 32-bit PE (i386)")

    def test_dll_fails(self):
        report = self.verify(WIN_CPU, build_pe(characteristics=0x2022))
        self.assertFailsWith(report, "image is a DLL")

    def test_only_a_program_passes(self):
        report = self.verify(WIN_CPU, build_pe(characteristics=0x0020))
        self.assertFailsWith(report, "IMAGE_FILE_EXECUTABLE_IMAGE is not set")
        self.assertFailsWith(self.verify(WIN_CPU, build_pe(entry_point=0)),
                             "entry point RVA is 0")

    def test_other_formats_and_truncation_fail_cleanly(self):
        for label, blob in (("elf", build_elf()), ("empty", b""), ("truncated", build_pe()[:0x100]),
                            ("no-pe-signature", b"MZ" + b"\0" * 0x80)):
            with self.subTest(blob=label):
                self.assertFailsWith(self.verify(WIN_CPU, blob), "not a valid PE executable")


class HashAndNameTests(TempDirTestCase):
    def lock_with(self, **hashes):
        return vb.Lock(**{**LOCK.__dict__, "sha256": hashes})

    def test_hash_match_passes(self):
        blob = build_elf()
        lock = self.lock_with(**{LINUX_CPU: hashlib.sha256(blob).hexdigest()})
        report = self.verify(LINUX_CPU, blob, lock=lock)
        self.assertPasses(report)
        self.assertIn("sha256 matches lock", report.facts)

    def test_hash_mismatch_fails(self):
        lock = self.lock_with(**{LINUX_CPU: "0" * 64})
        report = self.verify(LINUX_CPU, build_elf(), lock=lock)
        self.assertFailsWith(report, "sha256 mismatch")

    def test_unpinned_file_fails_when_lock_pins_hashes(self):
        lock = self.lock_with(**{LINUX: "0" * 64})
        self.assertFailsWith(self.verify(LINUX_CPU, build_elf(), lock=lock),
                             f"{LINUX_CPU} is not pinned in the lock")

    def test_hashless_lock(self):
        self.assertPasses(self.verify(LINUX_CPU, build_elf()))
        report = self.verify(LINUX_CPU, build_elf(), require_hashes=True)
        self.assertFailsWith(report, "the lock pins no hashes")

    def test_unrecognized_name_fails(self):
        for name in ("llama-server-cpu-aarch64-apple-darwin", "llama-server-x86_64-pc-windows-msvc",
                     "llama-server-x86_64-unknown-linux-gnu.exe", "llama-server"):
            with self.subTest(name=name):
                self.assertFailsWith(self.verify(name, build_elf()), "unrecognized file name")


@unittest.skipIf(os.name == "nt", "Windows file systems have no executable bit")
class ExecutableBitTests(TempDirTestCase):
    """A sidecar without +x only fails when the app first spawns it."""

    def test_missing_executable_bit_fails(self):
        for name in (MAC, MAC_INTEL, LINUX_CPU):
            with self.subTest(name=name):
                path = self.write(name, GOOD_BLOBS[name](), mode=0o644)
                self.assertFailsWith(vb.verify_file(path, LOCK), "not executable (mode -rw-r--r--)")

    def test_any_execute_bit_is_enough(self):
        for mode in (0o755, 0o700, 0o544):
            with self.subTest(mode=oct(mode)):
                self.assertPasses(vb.verify_file(self.write(LINUX_CPU, build_elf(), mode=mode), LOCK))

    def test_windows_targets_are_exempt(self):
        # Neither a Windows target nor an NTFS host has such a bit; requiring
        # one would only fail on the POSIX host that staged the release.
        for name in (WIN, WIN_CPU):
            with self.subTest(name=name):
                path = self.write(name, GOOD_BLOBS[name](), mode=0o644)
                self.assertPasses(vb.verify_file(path, LOCK))


class SingleSourceOfTruthTests(unittest.TestCase):
    """The file list lives in TARGETS; nothing else may repeat it."""

    REPO_ROOT = SCRIPT_DIR.parents[1]
    CONSUMERS = (SCRIPT_DIR / "fetch-llama-binaries.sh",
                 REPO_ROOT / ".github/workflows/llama-build.yml")

    def test_print_targets_covers_every_release_file(self):
        self.assertEqual(vb.print_targets("files").splitlines(), list(vb.EXPECTED_FILES))
        matrix = json.loads(vb.print_targets("matrix"))["include"]
        self.assertEqual([entry["file"] for entry in matrix], list(vb.EXPECTED_FILES))
        for entry in matrix:
            self.assertTrue(entry["os"] and entry["name"], entry)
            self.assertEqual(entry["vulkan"], vb.TARGETS[entry["file"]].uses_vulkan)
        self.assertEqual(len(vb.print_targets("markdown").splitlines()), len(vb.EXPECTED_FILES))

    def test_print_targets_writes_lf_only(self):
        """The fetch script matches these filenames against the lock exactly,
        so one platform-translated newline makes every binary look unpinned —
        which is what happened on Windows, where text mode turns "\\n" into
        "\\r\\n". Asserts on raw bytes, because `splitlines()` hides exactly
        the character that broke it. Run as a subprocess so the real stdout
        encoding is exercised rather than an in-process capture.
        """
        for fmt in ("files", "matrix", "markdown"):
            proc = subprocess.run(
                [sys.executable, str(SCRIPT_DIR / "verify_llama_binaries.py"),
                 "--print-targets", fmt],
                capture_output=True, check=True)
            self.assertNotIn(b"\r", proc.stdout, f"--print-targets {fmt} must be LF-only")

    def test_consumers_do_not_name_the_release_files(self):
        for path in self.CONSUMERS:
            with self.subTest(file=path.name):
                text = path.read_text(encoding="utf-8")
                for name in vb.EXPECTED_FILES:
                    self.assertNotIn(name, text, f"{path.name} keeps its own copy of {name}")

    def test_metal_bf16_is_tied_to_the_advertised_macos_floor(self):
        # Metal `bfloat` needs macOS 14; the shaders are compiled on the user's
        # machine, so a 13.x failure is invisible to every CI job.
        workflow = (self.REPO_ROOT / ".github/workflows/llama-build.yml").read_text(encoding="utf-8")
        major = vb.parse_version(vb.parse_lock(vb.DEFAULT_LOCK).macos_min)[0]
        if major < 14:
            self.assertNotIn("GGML_METAL_USE_BF16", vb.TARGETS[MAC].cmake_backend)
            if "GGML_METAL_USE_BF16" in workflow:
                self.assertIn('"${macos_min%%.*}" -ge 14', workflow)


class LockParserTests(TempDirTestCase):
    def parse(self, text: str) -> vb.Lock:
        path = self.dir / "llama-server.lock"
        path.write_text(text, encoding="utf-8")
        return vb.parse_lock(path)

    def assertRejected(self, text: str, fragment: str):
        with self.assertRaises(vb.LockError) as ctx:
            self.parse(text)
        self.assertIn(fragment, str(ctx.exception))

    def test_valid_lock(self):
        digest = "ab" * 32
        lock = self.parse(LOCK_TEXT + f"sha256 {digest} {MAC}\n  # indented comment\n")
        self.assertEqual((lock.llama_cpp_tag, lock.release, lock.repo, lock.macos_min, lock.glibc_max),
                         ("b1", "llama/b1-r1", "owner/repo", "13.3", "2.35"))
        self.assertEqual(lock.sha256, {MAC: digest.lower()})

    def test_repository_lock_parses(self):
        lock = vb.parse_lock(vb.DEFAULT_LOCK)
        self.assertTrue(lock.release)
        for name in lock.sha256:
            self.assertIn(name, vb.EXPECTED_FILES)

    def test_rejects_junk(self):
        cases = {
            "unknown key": (LOCK_TEXT + "platform macos\n", "unknown key 'platform'"),
            "bare word": (LOCK_TEXT + "junk\n", "unknown key 'junk'"),
            "missing key": (LOCK_TEXT.replace("glibc_max 2.35\n", ""), "missing required key(s): glibc_max"),
            "duplicate key": (LOCK_TEXT + "repo other/repo\n", "duplicate key 'repo'"),
            "extra token": (LOCK_TEXT.replace("repo owner/repo", "repo owner/repo # note"),
                            "expected `repo <value>`"),
            "no value": (LOCK_TEXT.replace("repo owner/repo", "repo"), "expected `repo <value>`"),
            "bad version": (LOCK_TEXT.replace("macos_min 13.3", "macos_min thirteen"),
                            "macos_min must be a dotted version"),
            "short hash": (LOCK_TEXT + f"sha256 abc {MAC}\n",
                           "not a lowercase 64-digit hex sha256"),
            "hash arity": (LOCK_TEXT + f"sha256 {'a' * 64}\n", "expected `sha256 <hex> <file>`"),
            "hash path": (LOCK_TEXT + f"sha256 {'a' * 64} bin/{MAC}\n", "bare file name"),
            "duplicate hash": (LOCK_TEXT + f"sha256 {'a' * 64} {MAC}\nsha256 {'b' * 64} {MAC}\n",
                               f"duplicate sha256 line for {MAC}"),
        }
        for label, (text, fragment) in cases.items():
            with self.subTest(case=label):
                self.assertRejected(text, fragment)

    def test_error_names_the_line(self):
        self.assertRejected(LOCK_TEXT + "bogus 1\n", "llama-server.lock:8:")

    def test_semantics_match_the_rust_guard(self):
        # build_support/sidecar_guard.rs::lock_semantics_match_the_python_verifier
        # pins the other half of this: a lock is valid for both parsers or for
        # neither, and they read the same meaning out of it.
        self.assertRejected(LOCK_TEXT + f"sha256 {'A' * 64} {MAC}\n", "lowercase")
        for text in ("13", "13.3", "13.3.0"):
            self.assertEqual(self.parse(self.written(f"macos_min {text}")).macos_min, text)
        for text in ("13.", "13.3.0.1", "thirteen", "13.3-beta", "v13"):
            with self.subTest(version=text):
                self.assertRejected(self.written(f"macos_min {text}"), "dotted version")
                self.assertRejected(self.written(f"glibc_max {text}"), "dotted version")
        # Trailing zeros are padding, not a newer version.
        equal = (vb.parse_version("13.3"), vb.parse_version("13.3.0"))
        self.assertTrue(vb.version_le(*equal) and vb.version_le(*reversed(equal)))
        self.assertFalse(vb.version_le(vb.parse_version("13.4"), vb.parse_version("13.3")))

    def written(self, line: str) -> str:
        """LOCK_TEXT with `line`'s key replaced by it."""
        key = line.split(" ", 1)[0]
        return "".join(line + "\n" if other.startswith(f"{key} ") else other + "\n"
                       for other in LOCK_TEXT.splitlines())

    def test_unreadable_lock(self):
        with self.assertRaises(vb.LockError):
            vb.parse_lock(self.dir / "missing.lock")
        path = self.dir / "binary.lock"
        path.write_bytes(b"\xff\xfe\x00junk")
        with self.assertRaises(vb.LockError):
            vb.parse_lock(path)


class VulkanLoaderClassificationTests(unittest.TestCase):
    LINUX_MISSING = ("./llama-server: error while loading shared libraries: libvulkan.so.1: "
                     "cannot open shared object file: No such file or directory\n")

    def test_linux_missing_loader(self):
        vulkan, cpu = vb.TARGETS[LINUX], vb.TARGETS[LINUX_CPU]
        self.assertTrue(vb.is_missing_vulkan_loader(vulkan, 127, self.LINUX_MISSING, False))
        self.assertFalse(vb.is_missing_vulkan_loader(cpu, 127, self.LINUX_MISSING, False))
        other = self.LINUX_MISSING.replace("libvulkan.so.1", "libstdc++.so.6")
        self.assertFalse(vb.is_missing_vulkan_loader(vulkan, 127, other, False))
        self.assertFalse(vb.is_missing_vulkan_loader(vulkan, 134, "Segmentation fault", False))
        self.assertFalse(vb.is_missing_vulkan_loader(vulkan, 0, self.LINUX_MISSING, False))
        self.assertFalse(vb.is_missing_vulkan_loader(vulkan, None, self.LINUX_MISSING, False))

    def test_windows_dll_not_found(self):
        vulkan, cpu = vb.TARGETS[WIN], vb.TARGETS[WIN_CPU]
        for code in (0xC0000135, 3221225781, -1073741515):
            with self.subTest(code=code):
                self.assertTrue(vb.is_missing_vulkan_loader(vulkan, code, "", False))
                self.assertFalse(vb.is_missing_vulkan_loader(vulkan, code, "", True))
                self.assertFalse(vb.is_missing_vulkan_loader(cpu, code, "", False))
        self.assertFalse(vb.is_missing_vulkan_loader(vulkan, 0xC0000005, "", False))
        self.assertFalse(vb.is_missing_vulkan_loader(vulkan, 1, "", False))

    def test_macos_never_qualifies(self):
        self.assertFalse(vb.is_missing_vulkan_loader(vb.TARGETS[MAC], 134, self.LINUX_MISSING, False))

    def test_windows_loader_search(self):
        root = "WINROOT"
        found = {os.path.join(root, "System32", "vulkan-1.dll"),
                 os.path.join("sdk", "bin", "vulkan-1.dll")}
        isfile = found.__contains__
        self.assertTrue(vb.windows_vulkan_loader_findable({"SystemRoot": root}, isfile))
        self.assertFalse(vb.windows_vulkan_loader_findable({"SystemRoot": "elsewhere"}, isfile))
        env = {"SystemRoot": "elsewhere", "PATH": os.pathsep.join(["", "nope", os.path.join("sdk", "bin")])}
        self.assertTrue(vb.windows_vulkan_loader_findable(env, isfile))


class RunCheckTests(TempDirTestCase):
    """check_run decisions, with the process launch replaced."""

    def run_check(self, name, result, host, findable=False, prior_failure=None):
        target = vb.TARGETS[name]
        report = vb.Report(path=self.dir / name, target=target)
        if prior_failure:
            report.fail(prior_failure)
        with mock.patch.object(vb, "run_isolated", return_value=result) as launched, \
                mock.patch.object(vb, "windows_vulkan_loader_findable", return_value=findable):
            vb.check_run(report.path, target, report, host)
        return report, launched

    def test_other_host_is_skipped_with_note(self):
        report, launched = self.run_check(MAC, vb.RunResult(0, "version: 1"), ("linux", "x86_64"))
        launched.assert_not_called()
        self.assertEqual(report.status, "PASS")
        self.assertEqual(report.notes, ["run skipped: host is x86_64 linux"])

    def test_success_records_version_line(self):
        output = "load_backend: ...\nversion: 8981 (abc123)\nbuilt with clang\n"
        report, _ = self.run_check(MAC, vb.RunResult(0, output), ("macos", "aarch64"))
        self.assertEqual(report.status, "PASS")
        self.assertIn("runs (version: 8981 (abc123))", report.facts)

    def test_missing_vulkan_loader_is_skip(self):
        result = vb.RunResult(127, VulkanLoaderClassificationTests.LINUX_MISSING)
        report, _ = self.run_check(LINUX, result, ("linux", "x86_64"))
        self.assertEqual(report.status, "SKIP")
        self.assertEqual(report.skipped, "Vulkan loader not installed on this host; static checks passed")

        report, _ = self.run_check(WIN, vb.RunResult(3221225781, ""), ("windows", "x86_64"))
        self.assertEqual(report.status, "SKIP")
        report, _ = self.run_check(WIN, vb.RunResult(3221225781, ""), ("windows", "x86_64"),
                                   findable=True)
        self.assertEqual(report.status, "FAIL")
        self.assertIn("exit code 3221225781 (0xc0000135)", report.failures[0])

    def test_missing_vulkan_loader_does_not_hide_static_failures(self):
        result = vb.RunResult(127, VulkanLoaderClassificationTests.LINUX_MISSING)
        report, _ = self.run_check(LINUX, result, ("linux", "x86_64"), prior_failure="DT_RPATH present")
        self.assertEqual(report.status, "FAIL")
        self.assertIn("run skipped: Vulkan loader not installed on this host", report.notes)

    def test_other_vulkan_failures_fail(self):
        result = vb.RunResult(127, "error while loading shared libraries: libstdc++.so.6: "
                                   "cannot open shared object file")
        report, _ = self.run_check(LINUX, result, ("linux", "x86_64"))
        self.assertEqual(report.status, "FAIL")

    def test_signal_names_do_not_depend_on_the_host(self):
        # Windows numbers SIGABRT 22, so the host's signal table cannot be used
        # to describe a return code that came from a Unix-style target.
        self.assertEqual(vb.describe_returncode(-6), "killed by signal 6 (SIGABRT)")
        self.assertEqual(vb.describe_returncode(-11), "killed by signal 11 (SIGSEGV)")
        self.assertEqual(vb.describe_returncode(-99), "killed by signal 99 (unknown signal)")
        self.assertEqual(vb.describe_returncode(3221225781),
                         "exit code 3221225781 (0xc0000135)")
        self.assertEqual(vb.describe_returncode(1), "exit code 1")

    def test_failure_output(self):
        output = "".join(f"line {i}\n" for i in range(1, 31))
        report, _ = self.run_check(LINUX_CPU, vb.RunResult(-6, output), ("linux", "x86_64"))
        text = report.failures[0]
        self.assertIn("run: `--version` failed: killed by signal 6 (SIGABRT)", text)
        self.assertIn("last 20 line(s) of output:", text)
        self.assertIn("| line 11\n", text)
        self.assertNotIn("| line 10\n", text)

        report, _ = self.run_check(LINUX_CPU, vb.RunResult(None, "partial", timed_out=True),
                                   ("linux", "x86_64"))
        self.assertIn("did not finish within 60s", report.failures[0])
        report, _ = self.run_check(LINUX_CPU, vb.RunResult(0, "usage: ..."), ("linux", "x86_64"))
        self.assertIn("exited 0 but printed no `version:` line", report.failures[0])
        report, _ = self.run_check(LINUX_CPU, vb.RunResult(None, "could not start: [Errno 8]"),
                                   ("linux", "x86_64"))
        self.assertEqual(report.failures, ["run: could not start: [Errno 8]"])

    def test_scrubbed_env(self):
        env = {"PATH": "/bin", "DYLD_LIBRARY_PATH": "x", "DYLD_INSERT_LIBRARIES": "y",
               "LD_LIBRARY_PATH": "z", "LD_PRELOAD": "w", "ld_library_path": "v", "OLD_X": "keep"}
        self.assertEqual(vb.scrubbed_env(env), {"PATH": "/bin", "OLD_X": "keep"})


@unittest.skipIf(os.name == "nt", "uses POSIX shell scripts as stand-in executables")
class RunIsolatedTests(TempDirTestCase):
    """Launch real processes: shell scripts named like a binary for this host."""

    def script(self, body: str, subdir: str = "src") -> Path:
        host = vb.host_platform()
        names = [name for name, target in vb.TARGETS.items()
                 if (target.os, target.arch) == host]
        if not names:
            self.skipTest(f"no release target for host {host}")
        path = self.dir / subdir / names[-1]
        path.parent.mkdir()
        path.write_text("#!/bin/sh\n" + textwrap.dedent(body))
        path.chmod(0o644)  # run_isolated must make its copy executable itself
        return path

    def test_runs_copy_in_empty_directory_without_loader_overrides(self):
        path = self.script("""\
            echo "version: 42 (deadbeef)"
            echo "ld=${LD_LIBRARY_PATH-unset} preload=${LD_PRELOAD-unset} keep=${KEEP_ME-unset}"
            echo "self=$0"
            ls -A
        """)
        (path.parent / "libggml.dylib").write_text("sibling that must not be copied")
        env = {"LD_LIBRARY_PATH": "/x", "LD_PRELOAD": "/y", "KEEP_ME": "1"}
        with mock.patch.dict(os.environ, env):
            result = vb.run_isolated(path)
        self.assertEqual(result.returncode, 0, result.output)
        lines = result.output.splitlines()
        self.assertEqual(lines[0], "version: 42 (deadbeef)")
        self.assertEqual(lines[1], "ld=unset preload=unset keep=1")
        self.assertNotEqual(Path(lines[2].split("=", 1)[1]).parent, path.parent)
        self.assertEqual(lines[3:], [path.name])

    def test_nonzero_exit_and_timeout(self):
        result = vb.run_isolated(self.script("echo broken; exit 3\n"))
        self.assertEqual((result.returncode, result.output), (3, "broken\n"))
        result = vb.run_isolated(self.script("exec sleep 5\n", subdir="sleeper"), timeout=0.5)
        self.assertTrue(result.timed_out)
        self.assertIsNone(result.returncode)

    def test_end_to_end_check_run(self):
        path = self.script('echo "version: 7 (cafe)"\n')
        target = vb.TARGETS[path.name]
        report = vb.Report(path=path, target=target)
        vb.check_run(path, target, report, vb.host_platform())
        self.assertIn("runs (version: 7 (cafe))", report.facts)


class CliTests(TempDirTestCase):
    def test_pinned_selection_preserves_current_release_but_all_requires_intel(self):
        names = [name for name in vb.EXPECTED_FILES if name != MAC_INTEL]
        self.populate(names)
        self.lock.write_text(LOCK_TEXT + "".join(
            f"sha256 {hashlib.sha256((self.bin / name).read_bytes()).hexdigest()} {name}\n"
            for name in names))
        code, out = self.main("--lock", str(self.lock), "--pinned-only", "--require-hashes",
                              "--expect-all", str(self.bin))
        self.assertEqual(code, 0, out)
        code, out = self.main("--lock", str(self.lock), "--require-hashes",
                              "--expect-all", str(self.bin))
        self.assertEqual(code, vb.EXIT_FAILED, out)
        self.assertIn(f"missing release file(s): {MAC_INTEL}", out)
        # An existing pin is still authoritative; filtering cannot bypass hashes.
        (self.bin / MAC).write_bytes(build_macho(minos=(13, 0)))
        code, out = self.main("--lock", str(self.lock), "--pinned-only", "--require-hashes",
                              "--expect-all", str(self.bin))
        self.assertEqual(code, vb.EXIT_FAILED, out)
        self.assertIn("sha256 mismatch", out)

    def test_pinned_selection_rejects_empty_lock(self):
        code, out = self.main("--lock", str(self.lock), "--pinned-only", "--print-targets", "files")
        self.assertEqual(code, vb.EXIT_USAGE, out)
        self.assertIn("nonempty checksums", out)

    def setUp(self):
        super().setUp()
        self.lock = self.dir / "test.lock"
        self.lock.write_text(LOCK_TEXT, encoding="utf-8")
        self.bin = self.dir / "binaries"
        self.bin.mkdir()

    def populate(self, names=vb.EXPECTED_FILES):
        for name in names:
            self.write(name, GOOD_BLOBS[name](), self.bin)
        for extra in ("SHA256SUMS.txt", "README.md", ".gitignore"):
            (self.bin / extra).write_text("not a binary\n")

    def main(self, *args):
        out = io.StringIO()
        err = io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            try:
                code = vb.main(list(args))
            except SystemExit as exc:  # argparse usage errors
                code = exc.code
        return code, out.getvalue() + err.getvalue()

    def test_all_good(self):
        self.populate()
        code, out = self.main("--lock", str(self.lock), "--expect-all", str(self.bin))
        self.assertEqual(code, 0, out)
        self.assertEqual(sum(line.startswith("PASS ") for line in out.splitlines()), 6, out)
        self.assertIn("6 passed, 0 failed, 0 skipped", out)
        self.assertIn("pins no sha256 hashes", out)

    def test_expect_all_reports_missing(self):
        self.populate([name for name in vb.EXPECTED_FILES if name != LINUX_CPU])
        code, out = self.main("--lock", str(self.lock), "--expect-all", str(self.bin))
        self.assertEqual(code, 1, out)
        self.assertIn(f"FAIL missing release file(s): {LINUX_CPU}", out)
        code, out = self.main("--lock", str(self.lock), str(self.bin))
        self.assertEqual(code, 0, out)

    def test_explicit_files_and_failures(self):
        self.populate()
        bad = self.write(MAC, build_macho(rpaths=("/opt/x",)), self.dir)
        code, out = self.main("--lock", str(self.lock), str(self.bin / LINUX), str(bad))
        self.assertEqual(code, 1, out)
        self.assertIn(f"PASS {self.bin / LINUX} [x86_64-unknown-linux-gnu, vulkan]", out)
        self.assertIn(f"FAIL {bad} [aarch64-apple-darwin, metal]", out)
        self.assertIn("    - LC_RPATH present (a self-contained binary needs none): /opt/x", out)

    def test_require_hashes(self):
        self.populate()
        code, out = self.main("--lock", str(self.lock), "--require-hashes", str(self.bin))
        self.assertEqual(code, 1, out)
        self.assertNotIn("pins no sha256 hashes; hash check skipped", out)

        pinned = LOCK_TEXT + "".join(
            f"sha256 {hashlib.sha256((self.bin / name).read_bytes()).hexdigest()} {name}\n"
            for name in vb.EXPECTED_FILES)
        self.lock.write_text(pinned, encoding="utf-8")
        code, out = self.main("--lock", str(self.lock), "--require-hashes", "--expect-all", str(self.bin))
        self.assertEqual(code, 0, out)

    def test_skip_only_exits_zero(self):
        self.write(LINUX, GOOD_BLOBS[LINUX](), self.bin)
        result = vb.RunResult(127, VulkanLoaderClassificationTests.LINUX_MISSING)
        with mock.patch.object(vb, "host_platform", return_value=("linux", "x86_64")), \
                mock.patch.object(vb, "run_isolated", return_value=result):
            code, out = self.main("--lock", str(self.lock), "--run", str(self.bin))
        self.assertEqual(code, 0, out)
        self.assertIn(f"SKIP {self.bin / LINUX}", out)
        self.assertIn("    skip: Vulkan loader not installed on this host; static checks passed", out)
        self.assertIn("0 passed, 0 failed, 1 skipped", out)

    def test_strict_fails_skipped_runs(self):
        self.write(LINUX, GOOD_BLOBS[LINUX](), self.bin)
        result = vb.RunResult(127, VulkanLoaderClassificationTests.LINUX_MISSING)
        with mock.patch.object(vb, "host_platform", return_value=("linux", "x86_64")), \
                mock.patch.object(vb, "run_isolated", return_value=result):
            code, out = self.main("--lock", str(self.lock), "--run", "--strict", str(self.bin))
        self.assertEqual(code, 1, out)
        self.assertIn("FAIL --strict: every binary for this host must run", out)

    def test_run_only_flags_are_never_silently_ignored(self):
        self.populate()
        for flag in ("--strict", "--require-run"):
            with self.subTest(flag=flag):
                code, out = self.main("--lock", str(self.lock), flag, str(self.bin))
                self.assertEqual(code, 2, out)
                self.assertIn(f"{flag} requires --run", out)

    def test_require_run_needs_an_executed_binary(self):
        # Pinning a hash is a promise that the build works, so the caller must
        # be able to insist that something really ran.
        self.write(LINUX, GOOD_BLOBS[LINUX](), self.bin)
        with mock.patch.object(vb, "host_platform", return_value=("macos", "aarch64")):
            code, out = self.main("--lock", str(self.lock), "--run", "--require-run", str(self.bin))
        self.assertEqual(code, 1, out)
        self.assertIn("FAIL --require-run: no binary was executed (host is aarch64 macos)", out)

        result = vb.RunResult(0, "version: 8981 (abc)")
        with mock.patch.object(vb, "host_platform", return_value=("linux", "x86_64")), \
                mock.patch.object(vb, "run_isolated", return_value=result):
            code, out = self.main("--lock", str(self.lock), "--run", "--require-run", str(self.bin))
        self.assertEqual(code, 0, out)

    def test_print_targets_needs_no_lock_or_paths(self):
        code, out = self.main("--lock", str(self.dir / "missing.lock"), "--print-targets", "files")
        self.assertEqual(code, 0, out)
        self.assertEqual(out.split(), list(vb.EXPECTED_FILES))
        self.assertEqual(self.main("--print-targets", "elvish")[0], 2)

    def test_empty_directory_fails(self):
        code, out = self.main("--lock", str(self.lock), str(self.bin))
        self.assertEqual(code, 1, out)
        self.assertIn("no llama-server-* binaries found", out)

    def test_usage_and_lock_errors_exit_2(self):
        self.populate()
        self.assertEqual(self.main("--lock", str(self.lock), str(self.dir / "nope"))[0], 2)
        self.assertEqual(self.main("--lock", str(self.dir / "missing.lock"), str(self.bin))[0], 2)
        self.lock.write_text(LOCK_TEXT + "surprise 1\n", encoding="utf-8")
        code, out = self.main("--lock", str(self.lock), str(self.bin))
        self.assertEqual(code, 2)
        self.assertIn("unknown key 'surprise'", out)
        self.assertEqual(self.main("--lock", str(self.lock))[0], 2)  # no PATH
        self.assertEqual(self.main("--bogus", str(self.bin))[0], 2)

    def test_script_runs_from_any_directory(self):
        elsewhere = self.dir / "elsewhere"
        elsewhere.mkdir()
        env = dict(os.environ, PYTHONDONTWRITEBYTECODE="1")
        proc = subprocess.run([sys.executable, str(SCRIPT), "--help"], cwd=elsewhere, env=env,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=60)
        self.assertEqual(proc.returncode, 0, proc.stdout)
        # The default lock next to the script is found; an empty directory
        # then fails with 1 (a missing lock would exit 2).
        proc = subprocess.run([sys.executable, str(SCRIPT), str(self.bin)], cwd=elsewhere, env=env,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=60)
        self.assertEqual(proc.returncode, 1, proc.stdout)
        self.assertIn(b"no llama-server-* binaries found", proc.stdout)


@unittest.skipIf(os.name == "nt", "the fetch script needs bash")
class FetchScriptFlagTests(unittest.TestCase):
    """fetch-llama-binaries.sh rejects flag combinations it cannot honour.

    These run the real script; every case is refused before it reads the lock
    or reaches the network.
    """

    SCRIPT = SCRIPT_DIR / "fetch-llama-binaries.sh"

    def run_script(self, *args):
        proc = subprocess.run(["bash", str(self.SCRIPT), *args], stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, timeout=60)
        return proc.returncode, proc.stdout.decode()

    def test_contradictory_flags_are_refused(self):
        cases = (
            (("--no-run", "--strict"), "cannot be combined with --no-run"),
            (("--update-lock", "--no-run"), "must run the binaries it pins"),
            (("--repin",), "--repin only applies to --update-lock"),
            (("--check", "--repin"), "--repin only applies to --update-lock"),
            (("--bogus",), "unknown argument"),
        )
        for args, fragment in cases:
            with self.subTest(args=args):
                code, out = self.run_script(*args)
                self.assertEqual(code, 2, out)
                self.assertIn(fragment, out)

    def test_help_documents_repin(self):
        code, out = self.run_script("--help")
        self.assertEqual(code, 0, out)
        self.assertIn("--repin", out)


@unittest.skipUnless(os.environ.get("VERIFY_LLAMA_REAL_BINARIES"),
                     "set VERIFY_LLAMA_REAL_BINARIES to directories of real release binaries")
class RealBinaryTests(unittest.TestCase):
    """Real binaries must at least parse; pass/fail depends on the build."""

    def test_real_binaries_parse(self):
        directories = [d for d in os.environ["VERIFY_LLAMA_REAL_BINARIES"].split(os.pathsep) if d]
        files = vb.collect_binaries(directories)
        self.assertTrue(files, "no llama-server-* files found")
        lock = vb.parse_lock(vb.DEFAULT_LOCK)
        for path in files:
            with self.subTest(file=str(path)):
                report = vb.verify_file(path, lock)
                sys.stderr.write(vb.format_report(report) + "\n")
                self.assertIsNotNone(report.target, vb.format_report(report))
                self.assertTrue(report.parsed, vb.format_report(report))


if __name__ == "__main__":
    unittest.main()
