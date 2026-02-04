# ShellWeGo Edge
**L7 Proxy.** A Traefik replacement written in Rust for sub-millisecond overhead.

- **Routing:** Dynamic rule-based matching (host, path, headers).
- **TLS:** Fully automated ACME/Let's Encrypt certificates.
- **Performance:** Built on `hyper` 1.0; supports HTTP/2 and WebSocket upgrades.
- **Load Balancing:** Round-robin and least-connection strategies.
