# whaleinit

A simple and lightweight init process for containers.  
It handles reaping zombie processes, propagates `SIGTERM` and `SIGINT` signals, and organizes log output.

## Example Service File

```toml
# filepath: /etc/whaleinit.toml

[[templates]]
src = "/etc/nginx/nginx.conf"
dest = "/etc/nginx/nginx.conf"

[[services]]
title = "nginx"
exec = "/usr/sbin/nginx"
args = ["-D"]
essential = true

[[services]]
title = "sshd"
exec = "/usr/sbin/sshd"
args = ["-D"]
```

## Template

In addition to the `[[templates]]` directive, the template system can also be used in the configuration file.
The template language is [Liquid](https://shopify.github.io/liquid/), and there is an `env` variable that can be used as a global variable to pull in environment variables.
