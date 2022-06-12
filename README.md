# 📬 portnord 

Simple port forwarding TUI for Kubernetes 💻↔⎈

![portnord in action](./media/portnord.mov)

## ✨ Features
- List all ports exposed by services in a given namespace.
- Toggle port forwarding for a specific port exposed by a service.
- Toggle port forwarding for all ports exposed by a service.

## 🧩 Usage

```
$ portnord --help

portnord 0.1.0

USAGE:
    portnord [OPTIONS]

OPTIONS:
    -h, --help                     Print help information
    -n, --namespace <NAMESPACE>    Point to a specific namespace ('default' otherwise)
    -V, --version                  Print version information
```

## 🐞 Limitations

This is a work in progress, and functionality is currently limited and unstable. (This is a pet project I'm working on to learn Rust! 🦀😍 Apologies if the code does not look idiomatic 📝🥺)

A partial list of limitations:
- The list of pods and services is not updated live.
- Eventual failure to port forward is not signaled to the user.
- Errors are currently printed to stdout, which breaks the TUI.

For most issues, restarting the app is the solution 🧸