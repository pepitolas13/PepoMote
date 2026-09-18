# Contributing to PepoMote

**English** · [Español](CONTRIBUTING.es.md)

Thanks for helping make PepoMote better. The aim is simple: pick up a phone, connect it and enjoy playing. A clear bug report, a test on a device we do not have, or a better explanation can be just as valuable as code. You do not need to be an experienced developer to take part.

**Write in English or Spanish, whichever is easier for you.** There is no need to translate your issue or pull request. Please follow our [Code of Conduct](CODE_OF_CONDUCT.md).

## Choose the right place

| You want to… | Start here |
| --- | --- |
| Get help installing, pairing or configuring a controller | [Support guide](SUPPORT.md), [Discussions](https://github.com/pepitolas13/PepoMote/discussions) or [Discord](https://discord.gg/Vx3MPuMPxb) |
| Report a reproducible problem | [Bug report](https://github.com/pepitolas13/PepoMote/issues/new?template=01-bug-en.yml) |
| Suggest an improvement | [Feature request](https://github.com/pepitolas13/PepoMote/issues/new?template=03-feature-en.yml) |
| Fix a guide, translation or accessibility barrier | [Documentation or accessibility issue](https://github.com/pepitolas13/PepoMote/issues/new?template=05-docs-accessibility.yml), or a small pull request directly |
| Report a security vulnerability | [Private reporting instructions](docs/SECURITY.md) |

Search existing issues and discussions first. If someone has already reported your problem, add your device details or reproduction steps there. A reaction is enough when you just want to support an existing idea.

## Useful contributions

- **Test real devices and games.** Tell us which phone, receiver, operating systems, PepoMote versions, emulator and core you used. Include both what works and what does not. Simulation tests cannot cover every combination.
- **Improve the first few minutes.** Clearer installation instructions, pairing help and controller labels make a real difference.
- **Improve accessibility and translations.** Describe the task that is difficult and how a change would help. For interface text, keep English and Spanish in sync where possible; ask for help if you only speak one of them.
- **Fix a focused problem.** Browse [good first issues](https://github.com/pepitolas13/PepoMote/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22) and [help wanted](https://github.com/pepitolas13/PepoMote/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22). If the lists are empty, describe what you would like to work on in Discussions.

For a large feature, a new dependency or a protocol change, open an issue before investing a lot of time. This helps us agree on the problem, platform support and maintenance cost. Small fixes and documentation improvements can go straight to a pull request.

## Find your way around

| Directory | What lives there |
| --- | --- |
| `desktop/` | Rust receiver for Windows, Linux and macOS |
| `android/` | Android controller and Android server, in Kotlin |
| `ios/` | iPhone and iPad controller, in Swift |
| `mobile-linux/` | Rust controller for Linux phones |
| `pmp/` | Shared Rust protocol codec |
| `protocol/` | Protocol documentation, test vectors and controller layouts |
| `docs/` | Setup guides, troubleshooting and release notes |
| `packaging/` | Platform packaging and installation scripts |

## Build and check your change

Fork the repository, clone your fork, create a branch from `main` and make one focused change. You only need the tools for the component you are changing. Documentation-only changes do not need an application build.

**Desktop and shared protocol:** install stable Rust. Windows also needs the MSVC C++ build tools; macOS needs Xcode command-line tools. Linux system packages are listed in the [desktop workflow](.github/workflows/desktop.yml). From the repository root:

```sh
cargo build --release --manifest-path desktop/Cargo.toml
cargo test --release --manifest-path desktop/Cargo.toml
cargo test --release --manifest-path pmp/Cargo.toml
```

For receiver or emulator integration changes, also follow the [isolated end-to-end test guide](desktop/e2e/README.md). Say which checks used simulated clients and which involved a real device or emulator.

**Android:** use JDK 17 and Android SDK 36 with build tools 36.0.0, matching the [Android workflow](.github/workflows/android.yml). From `android/`:

```sh
./gradlew testDebugUnitTest lintRelease assembleDebug
```

On Windows, use `./gradlew.bat testDebugUnitTest lintRelease assembleDebug` in PowerShell. Debug builds do not require the project's release signing keys.

**iOS:** use a Mac with Xcode 16 or later and XcodeGen. From `ios/`, run `xcodegen generate`, open `PepoMote.xcodeproj`, choose the `PepoMote` scheme and an available iPhone or iPad simulator, then run **Product → Test**. The [iOS workflow](.github/workflows/ios.yml) contains the command-line equivalent. Edit `project.yml` when changing project configuration; the Xcode project is generated.

**Linux phones:** use stable Rust on Linux and the system packages listed in the [mobile Linux workflow](.github/workflows/mobile-linux.yml). From the repository root:

```sh
cargo build --release --manifest-path mobile-linux/Cargo.toml
cargo test --release --manifest-path mobile-linux/Cargo.toml
cargo test --release --manifest-path pmp/Cargo.toml
```

If a tool, device or platform is unavailable to you, say so in the pull request. Include what you could verify and what still needs checking; an honest limitation is more useful than a guessed result.

## Send a pull request

1. Explain the problem and how the change improves it. Link an issue if there is one; creating an issue is not a requirement for a small fix.
2. Keep unrelated refactoring, generated files and release binaries out of the change. Follow the surrounding style.
3. For a bug fix, add or update a regression test where it can meaningfully cover the failure. Include relevant manual steps and results.
4. For interface changes, add a screenshot or short recording when useful, using your own content. Check English and Spanish, and phone and tablet layouts where affected.
5. For protocol or emulator configuration changes, consider all senders and receivers, compatibility with existing versions, and backup/restoration of user settings. Update [the protocol specification](protocol/PROTOCOL.md) and test vectors where needed.
6. Open the pull request against `main`. A draft is welcome if you want early feedback. List any untested platforms or remaining work.

Review may involve questions or a smaller first version. Features are assessed for usefulness, reliability, accessibility and long-term maintenance. PepoMote is maintained by Daniel (PepoTech); review and support happen as time allows, without a guaranteed response time.

## Respect people's data and other people's work

Remove pairing QR codes, tokens, passwords, private addresses and identifying details from logs and screenshots before sharing them. Never commit signing keys or credentials. Use the [security policy](docs/SECURITY.md) for vulnerabilities.

Only submit code and assets you have the right to contribute. The project uses GPL-3.0-or-later for code, CC-BY-SA 4.0 for its original visual and sound assets, and SIL OFL 1.1 for Nunito; see the [license](LICENSE) and [asset and attribution rules](docs/LEGAL.md). Keep third-party notices intact. Do not submit games, ROMs, BIOS files, proprietary console assets or links to unauthorized downloads.

Tools, including AI assistants, are welcome. You remain responsible for understanding your contribution, checking its sources and testing its behavior. If a tool limitation affects the review, explain it.

Thanks for taking the time to help. Even a small improvement can make someone's first game with PepoMote a better one.
