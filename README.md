# Yoink

Yoink is a forensic artefact collection tool that can be easily configured with YAML rules. It supports both Linux and Windows platforms with support for Android, IOS, and MacOS coming soon.

## Features

- Collect forensic artefacts based on configurable YAML rules.
- Supports both Linux and Windows platforms.
- Supports collection of arbitrary file streams on Windows
- Optionally encrypt the collected artefacts using AES256.

## Configuration

Yoink uses YAML files to define the rules for collecting artefacts. Below are examples of rules for both Windows and Linux.

### Example Rule for Windows

```
name: mft
description: Collects the Master File Table (MFT) from any NTFS file systems.
path: $MFT
platform: windows
```

### Example Rule for Linux

```
name: bash_history
description: Collects the bash history file for each user on the system which contains commands ran by the user.
path: "**/.bash_history"
platform: linux
```

## Building Yoink
To build Yoink, you need to have Rust installed, if you dont have it installed you can download it via the following link:

- https://rustup.rs/

Once installed, clone the repository and build the project:

```
git clone https://github.com/yourusername/yoink.git
cd yoink
cargo build --release
```

## Using Yoink

### Collection

```
Usage: yoink-cli.exe collect [OPTIONS] [RULES]...

Arguments:
  [RULES]...  the name of the rules to use for collection

Options:
  -l, --list
          list the rules that can be used for collection
  -r, --rule-dir <RULE_DIR>
          supply directory with custom rules [default: ]
  -a, --all
          use all rules for collection
  -e, --encryption-key <ENCRYPTION_KEY>
          encrypt the collection with a password using AES256 [default: ]
  -o, --output <OUTPUT>
          path the to the output file, must end in .zip e.g. /path/to/output.zip [default: DESKTOP-6K2FCE1_1732095047884]
  -h, --help
          Print help
  -V, --version
          Print version
```

To collect artefacts using all available rules and compress the collection:

```
yoink collect --all
```

To list available rules:

```
yoink collect --list
```

To use specific rules:

```
yoink collect --rules mft bash_history
```

To use custom rules from a directory:

```
yoink collect --rule-dir /path/to/custom/rules
```

To encrypt the collected artefacts using AES256, provide an encryption key:

```
yoink collect --all --encryption-key yourpassword
```