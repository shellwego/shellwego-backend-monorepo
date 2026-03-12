#!/usr/bin/env python3
"""
Script to split code blocks from docs/plan.patches.md into their correct file paths.
"""

import re 
import os
import sys
from pathlib import Path

def parse_patches_file(file_path):
    """Parse the patches markdown file and extract file paths and their code blocks."""
    
    with open(file_path, 'r') as f:
        content = f.read()
    
    # Split by the separator '---'
    sections = content.split('---')
    
    files = {}
    
    for section in sections:
        section = section.strip()
        if not section:
            continue
        
        # Extract ALL file paths and their code blocks in this section
        file_matches = list(re.finditer(r'\*\*`([^`]+)`\*\*', section))
        
        if not file_matches:
            continue
        
        # Find all code blocks and their positions
        code_matches = list(re.finditer(r'```(\w*)\n(.*?)\n```', section, re.DOTALL))
        
        # Pair file paths with code blocks
        for i, file_match in enumerate(file_matches):
            file_path = file_match.group(1)
            
            # Find the code block that follows this file path
            file_end = file_match.end()
            
            for code_match in code_matches:
                code_start = code_match.start()
                if code_start > file_end:
                    # This code block follows the file path
                    code_content = code_match.group(2)
                    files[file_path] = code_content
                    break
    
    return files

def create_files(files, base_dir='.'):
    """Create files with their content. Merges with existing files."""
    
    created = []
    merged = []
    errors = []
    
    for file_path, content in files.items():
        full_path = Path(base_dir) / file_path
        
        # Create parent directories if they don't exist
        full_path.parent.mkdir(parents=True, exist_ok=True)
        
        try:
            if full_path.exists():
                # Merge: read existing content, then apply new content
                with open(full_path, 'r') as f:
                    existing_content = f.read()
                
                # Simple merge strategy: replace existing content with new content
                # (since patches contain complete file contents)
                # TODO: Implement more sophisticated merge if needed
                with open(full_path, 'w') as f:
                    f.write(content)
                merged.append(file_path)
                print(f"Merged: {file_path}")
            else:
                with open(full_path, 'w') as f:
                    f.write(content)
                created.append(file_path)
                print(f"Created: {file_path}")
        except Exception as e:
            errors.append((file_path, str(e)))
            print(f"Error processing {file_path}: {e}")
    
    return created, merged, errors

def main():
    # Default path to the patches file
    patches_file = Path(__file__).parent.parent / 'docs' / 'plan.patches.md'
    
    # Allow override via command line
    if len(sys.argv) > 1:
        patches_file = Path(sys.argv[1])
    
    if not patches_file.exists():
        print(f"Error: Patches file not found: {patches_file}")
        sys.exit(1)
    
    print(f"Parsing: {patches_file}")
    print("-" * 60)
    
    # Parse the file
    files = parse_patches_file(patches_file)
    
    if not files:
        print("No code blocks found!")
        sys.exit(1)
    
    print(f"Found {len(files)} code blocks")
    print("-" * 60)
    
    # Create files
    base_dir = Path(__file__).parent.parent
    created, merged, errors = create_files(files, base_dir)
    
    print("-" * 60)
    print(f"Summary: {len(created)} files created, {len(merged)} files merged, {len(errors)} errors")
    
    if errors:
        print("\nErrors:")
        for file_path, error in errors:
            print(f"  {file_path}: {error}")
        sys.exit(1)

if __name__ == '__main__':
    main()
