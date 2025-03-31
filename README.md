# whaleinit

A simple and lightweight init process for containers.  
It handles reaping zombie processes, propagates `SIGTERM` and `SIGINT` signals, and organizes log output.

- `/etc/whaleinit/services/*.toml`
    - Service definitions

## Example Service File

```toml
exec = "/usr/sbin/nginx"
args = ["-D"]
```
