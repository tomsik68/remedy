# remedy

Remedy is a multi-threaded rust-imap-maildir synchronization program.

Please note that remedy is under heavy development.

Current features:
- IMAP
- TLS
- maildir
- configurable via toml (see `config.example.toml`)
- multiple accounts
- basic logging so it's possible to see what's going on
- use a shell command to retrieve account password

Missing features:
- save local state to prevent synchronization of already downloaded e-mails
- other formats
