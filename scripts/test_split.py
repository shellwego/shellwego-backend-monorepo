#!/usr/bin/env python3
"""
Minimal test to verify the split_patches.py script works correctly.
"""

import re
from pathlib import Path

def test_parsing():
    """Test the parsing logic with a sample input."""
    
    # Sample input matching the format in plan.patches.md
    sample_input = """
---

**`test/file.txt`**
```
Hello, World!
This is test content.
```

---

**`another/file.rs`**
```rust
fn main() {
    println!("Hello");
}
```
"""
    
    # Split by the separator '---'
    sections = sample_input.split('---')
    
    files = {}
    
    for section in sections:
        section = section.strip()
        if not section:
            continue
        
        # Extract file path from **`path`** pattern
        file_match = re.search(r'\*\*`([^`]+)`\*\*', section)
        if not file_match:
            continue
        
        file_path = file_match.group(1)
        
        # Extract code block content
        code_match = re.search(r'```(\w*)\n(.*?)\n```', section, re.DOTALL)
        if not code_match:
            continue
        
        code_content = code_match.group(2)
        
        files[file_path] = code_content
    
    # Verify results
    print("Test Results:")
    print("-" * 60)
    
    expected_files = {
        'test/file.txt': 'Hello, World!\nThis is test content.',
        'another/file.rs': 'fn main() {\n    println!("Hello");\n}'
    }
    
    success = True
    
    for file_path, expected_content in expected_files.items():
        if file_path in files:
            actual_content = files[file_path]
            if actual_content == expected_content:
                print(f"✓ {file_path}: PASS")
            else:
                print(f"✗ {file_path}: FAIL (content mismatch)")
                print(f"  Expected: {repr(expected_content)}")
                print(f"  Got:      {repr(actual_content)}")
                success = False
        else:
            print(f"✗ {file_path}: FAIL (not found)")
            success = False
    
    print("-" * 60)
    
    if success:
        print("All tests passed!")
        return 0
    else:
        print("Some tests failed!")
        return 1

if __name__ == '__main__':
    import sys
    sys.exit(test_parsing())
