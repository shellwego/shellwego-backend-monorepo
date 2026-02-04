# ShellWeGo Core
**The Shared Kernel.** Domain entities and types used across the workspace.

- **Single Source of Truth:** All App, Node, and Volume types live here.
- **Validation:** Strict input sanitization using the `validator` crate.
- **ORM Integration:** Optional `sea-orm` macros to keep entities lean.
- **Zero Logic:** Pure data structures for maximum portability.
