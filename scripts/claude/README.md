# Claude Host Scripts

These are transitional shell helpers moved from Lamella while equivalent
behavior continues to converge into `stipe doctor`, `stipe host doctor
claude-code`, and future host repair flows.

Use these only as manual fallback helpers:

- `check-claude.sh`: basic Claude CLI and environment health check
- `clean-reinstall-claude.sh`: destructive uninstall and reinstall helper

The long-term direction is to absorb this behavior into Stipe's Rust command
surface and retire the shell scripts.
