# Security policy

**English** · [Español](SECURITY.es.md)

PepoMote can send input to another device and, in some modes, receive screen content. Please treat pairing details as secrets and use the app on networks you trust. This page explains both how to report a vulnerability and the limits of the current security model.

## Report a vulnerability privately

Use **[Report a vulnerability on GitHub](https://github.com/pepitolas13/PepoMote/security/advisories/new)**. This opens a private report to the repository maintainer.

You can also contact **Daniel (PepoTech)** at **[pepo@pepotech.es](mailto:pepo@pepotech.es)**, preferably with the subject `PepoMote security`, or by **private message to PepoTech in [Discord](https://discord.gg/Vx3MPuMPxb)**. Email is the fallback if GitHub's reporting form is unavailable.

Please avoid public issues, pull requests or Discord channels for an unpatched vulnerability. Ordinary crashes, setup questions and feature requests belong in [the support channels](../SUPPORT.md), unless they have a security impact.

Include what you know; you do not need a complete exploit to raise a concern:

- Affected PepoMote version or commit, device and operating system, for both controller and receiver when relevant.
- A description of the problem, its possible impact and the access an attacker would need.
- Steps to reproduce it safely, using your own devices and test data.
- A minimal example, relevant log excerpt or proposed fix, if available.

Do not send real pairing tokens, passwords, private keys or other people's data. Use clearly marked test values. Only test systems you own or have permission to assess.

## Versions and follow-up

Security fixes are focused on the **latest published release**. Older releases do not have a guaranteed backport policy; please check whether a problem also affects the current release if you can do so safely. Reports about development code are welcome when they identify the commit. Code on `main` may contain changes that have not been released.

The maintainer will review the report, ask for any missing details and coordinate a fix and disclosure where appropriate. This is an independent project, with **no guaranteed response or resolution time**. If you have not heard back, follow up in the same private report or by email. Reporter credit can be agreed with you; your personal details will not be published without your permission. There is no paid bug bounty program.

## Current security model

### What pairing protects

The receiver uses a random pairing token, shared through the QR code and stored by the paired devices. It helps prevent accidental or casual unauthorized connections from other devices on the local network. The desktop receiver also supports a temporary, one-use pairing code; see the [protocol specification](../protocol/PROTOCOL.md).

### What pairing does not protect

- **PepoMote does not encrypt its application traffic.** Pairing and control traffic travel in cleartext over TCP/UDP on the network between the devices.
- Someone able to inspect that traffic may read it and capture a token. With that token and network access, they may be able to impersonate a controller and inject input.
- Pairing is not cryptographic authentication of the other device. Input data and any streamed screen content should not be treated as confidential on an untrusted network.

A configured VPN may protect traffic inside its tunnel, but it does not add encryption or peer authentication to PepoMote itself. Its protection depends on the VPN and network configuration.

### Practical precautions

- Use a trusted home Wi-Fi network secured with WPA2/WPA3 or your own private hotspot. Avoid open or shared networks such as hotels, campuses and offices unless you have an appropriately protected setup.
- The receiver may listen on all network interfaces. Allow only the network access your setup needs; on Windows, grant firewall access for **private networks**. Do not expose PepoMote or emulator input ports directly to the internet.
- Keep pairing QR codes, tokens and temporary codes private. Redact them, along with personal details, before sharing screenshots, configuration files or diagnostics.
- Keep both devices up to date and obtain PepoMote from the [official releases](https://github.com/pepitolas13/PepoMote/releases).

Application-level encryption and stronger peer authentication are areas for improvement, **not features of the current protocol**. See [how to contribute](../CONTRIBUTING.md) if you would like to help with that work.
