# Dictator Bot

Rust Discord bot for OS CVE alerts, OS release updates, snippets, and quick AI ask.

## Requirements

- Rust stable toolchain
- Discord bot token
- Message Content Intent enabled in Discord Developer Portal for snippet commands like `/ls`, `/sc`, `/name`

## Run

```powershell
copy .env.example .env
# edit .env
cargo run --release
```

## Slash Commands

- `/setup cve channel:#channel` sets the CVE update channel.
- `/cves now` checks CVEs immediately.
- `/cves ls` lists recent CVEs already seen by the bot.
- `/setup update channel:#channel` sets the OS release update channel.
- `/update now` checks OS releases immediately.
- `/snippet view name:name` views a snippet or alias.
- `/snippet list` lists snippets.
- `/snippet create name:name content:text` creates a snippet.
- `/snippet edit name:name content:text` edits a snippet.
- `/snippet info name:name` shows snippet info.
- `/snippet alias target:name alias:name` creates an alias.
- `/snippet lock name:name` locks or unlocks a snippet.
- `/snippet delete name:name` deletes a snippet.
- `/ask` posts a quick AI web panel that expires after 30 seconds. It does not use an AI API key.

Slash commands are registered globally when the bot starts. Discord can take a few minutes to show global command changes.

## Snippet Commands

The main snippet UI is `/snippet ...`. The bot also keeps quick text shortcuts for fast use:

- `/[name]` view snippet or alias.
- `/ls` list snippets.
- `/a [target] [alias]` create an alias.
- `/si [name]` view snippet info.
- `/sc [name] [content]` create snippet.
- `/se [name] [content]` edit snippet.
- `/sl [name]` lock/unlock snippet, owner only.
- `/sd [name]` or `sd [name]` delete snippet, owner or admin only.

## Data Sources

- CVEs: NVD CVE API v2.0, filtered to Linux, Windows, macOS, and BSD.
- OS releases: endoflife.date API for Windows, macOS, Ubuntu, Debian, RHEL, FreeBSD, OpenBSD, and NetBSD.
Made by Dictator Of Cake
Discord Server: https://discord.gg/SYhjR2bC2C or https://discord.gg/xwYRwwBBJT
There is some bugs ab module and function


The bot stores seen CVEs/releases in SQLite, so scheduled checks stay silent when there is nothing new.

## Quick Ask Note

Discord embeds cannot render an interactive website or web form inside chat. The bot posts an embed with buttons that open AI websites directly, then deletes the embed after 30 seconds.
Some bugs:
PS C:\Dictator bot> cargo run
   Compiling dictator-bot v0.1.0 (C:\Dictator bot)
error[E0425]: cannot find function `fetch_recent` in module `cve`
   --> src\commands.rs:172:22
    |
172 |     let items = cve::fetch_recent(&state.http, 48).await?;
    |                      ^^^^^^^^^^^^ not found in `cve`

error[E0277]: the size for values of type `[CveItem]` cannot be known at compilation time
   --> src\commands.rs:172:9
    |
172 |     let items = cve::fetch_recent(&state.http, 48).await?;
    |         ^^^^^ doesn't have a size known at compile-time
    |
    = help: the trait `Sized` is not implemented for `[CveItem]`
    = note: all local variables must have a statically known size

error[E0277]: the size for values of type `[CveItem]` cannot be known at compilation time
   --> src\commands.rs:172:17
    |
172 |     let items = cve::fetch_recent(&state.http, 48).await?;
    |                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ doesn't have a size known at compile-time
    |
    = help: the trait `Sized` is not implemented for `[CveItem]`
    = note: all local variables must have a statically known size

error[E0277]: the size for values of type `[CveItem]` cannot be known at compilation time
   --> src\commands.rs:172:57
    |
172 |     let items = cve::fetch_recent(&state.http, 48).await?;
    |                                                         ^ doesn't have a size known at compile-time
    |
    = help: the trait `Sized` is not implemented for `[CveItem]`
note: required by an implicit `Sized` bound in `ControlFlow`
   --> C:\Users\TD_999\.rustup\toolchains\stable-x86_64-pc-windows-msvc\lib/rustlib/src/rust\library\core\src\ops\control_flow.rs:89:25
    |
 89 | pub enum ControlFlow<B, C = ()> {
    |                         ^^^^^^ required by the implicit `Sized` requirement on this type parameter in `ControlFlow`

error[E0277]: the size for values of type `[CveItem]` cannot be known at compilation time
   --> src\commands.rs:172:17
    |
172 |     let items = cve::fetch_recent(&state.http, 48).await?;
    |                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ doesn't have a size known at compile-time
    |
    = help: the trait `Sized` is not implemented for `[CveItem]`
note: required by an implicit `Sized` bound in `ControlFlow`
   --> C:\Users\TD_999\.rustup\toolchains\stable-x86_64-pc-windows-msvc\lib/rustlib/src/rust\library\core\src\ops\control_flow.rs:89:25
    |
 89 | pub enum ControlFlow<B, C = ()> {
    |                         ^^^^^^ required by the implicit `Sized` requirement on this type parameter in `ControlFlow`

Some errors have detailed explanations: E0277, E0425.
For more information about an error, try `rustc --explain E0277`.
error: could not compile `dictator-bot` (bin "dictator-bot") due to 5 previous errors
