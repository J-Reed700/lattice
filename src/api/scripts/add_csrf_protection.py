"""
Script to add CSRF protection to all state-changing endpoints.
This adds 'csrf: None = Depends(csrf_protect)' to POST/PUT/PATCH/DELETE endpoints.
"""

import re
from pathlib import Path
from typing import List, Tuple

API_DIR = Path(__file__).parent.parent / "src" / "api" / "v1"

CSRF_IMPORT = "from src.middleware.csrf import csrf_protect"

EXEMPT_ENDPOINTS = {
    "/auth/register",
    "/auth/token",
    "/health",
}

def should_exempt_file(file_path: Path) -> bool:
    """Check if file should be exempted from CSRF protection."""
    return file_path.name in ["auth.py", "health.py", "csrf.py"]

def has_csrf_import(content: str) -> bool:
    """Check if file already has CSRF import."""
    return "from src.middleware.csrf import csrf_protect" in content

def add_csrf_import(content: str) -> str:
    """Add CSRF import to file if not present."""
    if has_csrf_import(content):
        return content
    
    lines = content.split("\n")
    last_import_idx = 0
    
    for idx, line in enumerate(lines):
        if line.startswith("from ") or line.startswith("import "):
            last_import_idx = idx
    
    lines.insert(last_import_idx + 1, CSRF_IMPORT)
    return "\n".join(lines)

def find_endpoint_functions(content: str) -> List[Tuple[int, str]]:
    """Find all endpoint function definitions with their line numbers."""
    pattern = r"@router\.(post|put|patch|delete)\([^\)]*\)"
    endpoints = []
    
    lines = content.split("\n")
    for idx, line in enumerate(lines):
        if re.search(pattern, line):
            endpoints.append((idx, line))
    
    return endpoints

def has_csrf_dependency(func_def: str) -> bool:
    """Check if function already has CSRF protection."""
    return "csrf_protect" in func_def or "csrf: " in func_def

def add_csrf_to_function(lines: List[str], decorator_idx: int) -> List[str]:
    """Add CSRF dependency to function parameters."""
    func_idx = decorator_idx + 1
    while func_idx < len(lines) and not lines[func_idx].strip().startswith("def "):
        func_idx += 1
    
    if func_idx >= len(lines):
        return lines
    
    func_line = lines[func_idx]
    if has_csrf_dependency(func_line):
        return lines
    
    paren_count = 0
    param_end_idx = func_idx
    
    for idx in range(func_idx, len(lines)):
        paren_count += lines[idx].count("(") - lines[idx].count(")")
        if paren_count == 0:
            param_end_idx = idx
            break
    
    if ") ->" in lines[param_end_idx]:
        lines[param_end_idx] = lines[param_end_idx].replace(
            ") ->",
            ",\n    csrf: None = Depends(csrf_protect)\n) ->"
        )
    elif "):" in lines[param_end_idx]:
        lines[param_end_idx] = lines[param_end_idx].replace(
            "):",
            ",\n    csrf: None = Depends(csrf_protect)\n):"
        )
    
    return lines

def process_file(file_path: Path) -> Tuple[bool, str]:
    """Process a single file to add CSRF protection."""
    if should_exempt_file(file_path):
        return False, f"Skipped (exempt): {file_path.name}"
    
    content = file_path.read_text(encoding="utf-8")
    
    endpoints = find_endpoint_functions(content)
    if not endpoints:
        return False, f"No endpoints found: {file_path.name}"
    
    content = add_csrf_import(content)
    lines = content.split("\n")
    
    modified_count = 0
    for decorator_idx, decorator_line in endpoints:
        old_lines = lines.copy()
        lines = add_csrf_to_function(lines, decorator_idx)
        if lines != old_lines:
            modified_count += 1
    
    if modified_count > 0:
        file_path.write_text("\n".join(lines), encoding="utf-8")
        return True, f"Modified {modified_count} endpoints in {file_path.name}"
    
    return False, f"No changes needed: {file_path.name}"

def main():
    """Main function to process all API files."""
    print("Adding CSRF protection to all state-changing endpoints...\n")
    
    api_files = list(API_DIR.glob("*.py"))
    api_files = [f for f in api_files if f.name not in ["__init__.py", "csrf.py"]]
    
    modified_files = []
    skipped_files = []
    
    for file_path in api_files:
        modified, message = process_file(file_path)
        print(message)
        
        if modified:
            modified_files.append(file_path.name)
        else:
            skipped_files.append(file_path.name)
    
    print(f"\n=== Summary ===")
    print(f"Modified: {len(modified_files)} files")
    print(f"Skipped: {len(skipped_files)} files")
    
    if modified_files:
        print("\nModified files:")
        for filename in modified_files:
            print(f"  - {filename}")

if __name__ == "__main__":
    main()
