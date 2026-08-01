# Security

Please report security issues privately to the repository maintainer instead of
opening a public issue. Include the affected version, operating system, and a
minimal reproduction. Do not include display serial numbers, usernames, or full
configuration files unless they are necessary and redacted.

Monitor Switch is local-only and does not open network ports or transmit usage
data. Its native backend communicates with attached displays through operating
system DDC/CI APIs. Compatibility backends execute locally installed command-line
tools only when explicitly selected in the configuration or CLI.
