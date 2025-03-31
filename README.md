# whaleinit

A simple and lightweight init process for containers.  
It handles reaping zombie processes, propagates `SIGTERM` and `SIGINT` signals, and organizes log output.

- `/etc/whaleinit/services/*.toml`
    - Service definitions

## Example Service File

```toml
# filepath: /etc/whaleinit.toml

[[services]]
title = "nginx"
exec = "/usr/sbin/nginx"
args = ["-D"]

[[services]]
title = "sshd"
exec = "/usr/sbin/sshd"
args = ["-D"]
```
