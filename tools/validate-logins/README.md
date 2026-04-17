# Validate Logins

Scratch tooling for origin validation used to investigate how login validation
behaves against real-world data from telemetry.

## Input format

The tool reads newline-delimited JSON (NDJSON) from stdin. Each line is one login entry:

```json
{"origin":"https://example.com","form_action_origin":"example.com","password":"p","username":"u","username_field":"u","password_field":"p"}
```

Real-world origins vary widely — bare hostnames (`ftp.example.com`), missing schemes
(`example.com`), FTP/SSH entries from legacy extensions (eg FireFTP), and
`chrome://` origins are all common. The validator checks and attempts to fix each one.
