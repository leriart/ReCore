# Discord in ReCore

| Area | Works on Linux | Open |
| --- | --- | --- |
| [Linking](#account-linking) | QR code or email and password from Settings, or the sign-in of a build without Matrix; several linked accounts, switched in Settings; unlinking one, with another taking its place; Discord's captcha, in the browser | Verify the reported two-captcha ticket rejection and the email login against real Discord |
| [Voice](voice.md) | Server voice channels and [stage channels](voice.md#stage-channels), [a card of who is in one](voice.md#who-is-in-a-voice-channel), [how long each one's call has gone on](voice.md#how-long-a-channels-call-has-gone-on), mute, deafen, per-person volume, [moving people to another channel](servers.md#someones-menu-in-a-voice-channel) from their menu or [by dragging them](servers.md#dragging-someone-to-another-voice-channel), voice gate with its sensitivity, [push-to-talk](voice.md#push-to-talk) and [keybinds](../keybinds.md) for mute, deafen and the input mode, [DeepFilterNet noise suppression](voice.md#microphone) and its strength, taking a room's echo out, [camera video](voice.md#camera-video), [the soundboard](voice.md#soundboard) | A check of Windows audio, cameras and keybinds on real Windows, and of keybinds on real Wayland desktops |
| [DM calls](voice.md#dm-calls) | Starting, ringing, answering and declining DM and group DM calls, with cameras | A check by hand through real Discord |
| [Go Live](go-live.md) | Sharing a screen with its sound, watching streams | A check on real Windows |
| [Channels and messages](messages.md) | History, live updates, sending, replies, edits, reactions, files, sound files that play in place, embeds with sensitive media covered, custom emoji, stickers, spoilers, unread state and [marking a message unread](messages.md#mark-unread-and-message-links), notifications and their settings, member lists, pinning messages, pins and search, reporting a message and deleting recent messages at once, a channel's topic and header, server, category and [channel menus](servers.md#a-channels-menu), [forwarding](messages.md#forwarding), pinning channels to the top, [following channels](servers.md#following-channels), invites, [joining by invite](servers.md#joining-by-invite) with its membership form and onboarding questions, invite cards in messages, [server folders](servers.md#server-folders), [scheduled events from their links](servers.md#event-links), Discord's own channel, message and profile links, making and changing channels, [server settings](server-settings.md)' Server Profile, Engagement, Boost Perks, [Emoji](server-settings.md#emoji), [Stickers](server-settings.md#stickers), [Members](server-settings.md#members), [Roles](server-settings.md#roles), [Invites](server-settings.md#invites), [Soundboard](server-settings.md#soundboard), [Webhooks](server-settings.md#webhooks), [Integrations](server-settings.md#integrations), [AutoMod](server-settings.md#automod), [Privacy](server-settings.md#privacy), [Discovery](server-settings.md#discovery), [Safety Setup](server-settings.md#safety-setup), [Audit Log](server-settings.md#audit-log), [Bans](server-settings.md#bans) and [Delete Server](server-settings.md#delete-server-and-transfer-ownership), [channel settings](server-settings.md#channel-settings) and their permissions, [two-factor authentication](server-settings.md#two-factor-authentication), threads, a channel's list of them and starting them, [forums](servers.md#forums), leaving servers, [bots' buttons, dropdown menus and forms](messages.md#bots-buttons-menus-and-forms), their radio groups, checkboxes and file uploads included, [apps' slash commands](messages.md#apps-commands), [context-menu commands](messages.md#apps) and [who used one](messages.md#who-used-an-apps-command) | The rest of a server's settings |
| [DMs](people.md#dms) | DMs, group DMs, message requests and spam, starting a DM, [a DM's menu](people.md#a-dms-menu): pinning, muting, closing and leaving a group | |
| [Friends](people.md#friends-and-requests) and [profiles](people.md#profiles) | Friends list, friend requests, adding by username, profile cards with their badges, others' server tags and their servers' cards, the DM panel, custom status, [editing one's profile](people.md#editing-ones-main-profile), with its theme, name style and server tag, and [server profiles](people.md#editing-ones-server-profile), notes, friend nicknames, removing friends, ignoring, blocking and unblocking, also [from a profile](people.md#profiles), [the blocked list](people.md#friends-and-requests) | A list of the ignored; changing the username |
| [Mirroring](#mirroring-into-matrix) | | Planned and not started |

Each area has a doc of its own: [voice](voice.md) and calls, [Go
Live](go-live.md), [channels and messages](messages.md), [servers](servers.md)
with their menus, invites, channels, threads and forums, [server
settings](server-settings.md) with channel settings and two-factor
authentication, and [DMs, friends and profiles](people.md) with presence, game
detection and Spotify. The [later list](#later) has what's left to do, and
[adding a feature](adding-a-feature.md) walks a new one through each layer.

ReCore connects to Discord directly, so a person who links their Discord account
doesn't need the Discord desktop app. ReCore joins Discord voice as that person,
shows their servers, channels and DMs, and sends and receives messages. ReCore must still be complete without Discord: nothing
Discord-related appears until someone links an account, and Discord work doesn't
replace beta features of ReCore itself.

Protocol references: Discord doesn't document what its own client sends. The
community's
[discord-userdoccers](https://github.com/discord-userdoccers/discord-userdoccers)
does, for example in `pages/gateway/gateway-events.mdx`,
`pages/topics/read-state.mdx`, `pages/resources/relationships.mdx`,
`pages/remote-authentication/desktop.mdx` and `pages/topics/cloud-uploads.mdx`.
The settings protobuf is in
[discord-protos](https://github.com/discord-userdoccers/discord-protos).

## Risk to accounts

Discord's terms don't allow third-party clients to use personal accounts.
Bans are uncommon, but they happen, and Discord can change its protocol and
break ReCore at any time. Linking is opt-in per person. ReCore shows this warning
before linking, and each person decides for their own account.

Behave like the official client to keep the risk low. Respect rate limits,
never act without a user action behind it, and don't poll. Keep all Discord code
in one crate, so a protocol change breaks only that part and ReCore can disable it
without affecting Matrix.

### Staying within Discord's limits

ReCore follows the rules Discord documents for every client:

- **HTTP rate limits.** Each request's route maps to the bucket Discord names in
  `X-RateLimit-Bucket`. Discord counts a bucket apart for each major ID, the
  channel, server or webhook, so ReCore keys its count by both: sending in one
  channel doesn't hold back another. ReCore waits for an empty bucket to reset
  before it asks again. It also waits after a global 429, marked so in the
  headers or the body. If Discord still answers 429, ReCore waits the body's
  `retry_after`, or `Retry-After`, or a second when Discord names neither, and
  tries once more. A wait longer than a minute fails at once, with a message to
  try again later.
- **Invalid requests.** Discord bans an IP that sends 10,000 requests in a
  10-minute window answered 401, 403 or 429. ReCore doesn't ask for what it knows
  will be refused: a channel the account's roles hide shows a notice instead
  of fetching messages.
- **Gateway resume.** When the connection drops, ReCore resumes the session at
  READY's `resume_gateway_url` with op 6 and the last sequence number, and
  Discord replays what was missed. ReCore also resumes after op 7, after op 9
  with `d: true`, and when a heartbeat goes unacknowledged. It identifies again
  only when Discord won't resume: op 9 with `d: false`, close codes 4007 and
  4009, or three failed attempts. After op 9 it waits a random one to five
  seconds first, or longer when sessions keep ending. Opening a connection,
  Discord's HELLO and each send give up after 10 seconds, so a dead connection
  fails rather than hangs. ReCore drops a connection without a close frame,
  because a close with 1000 or 1001 ends the session.
- **Gateway send limit.** A connection may send 120 commands a minute and is
  closed with 4008 past that. ReCore holds commands back at 110, so heartbeats
  always fit. The connection runs in a task of its own, so heartbeats go on
  time while commands wait.
- **Backoff.** Reconnects back off from 2 to 60 seconds, and start over only
  once a session has stayed up for a minute. Each wait is a random 50% to 100%
  of the step, so clients cut off together don't return together. ReCore sends
  at most 10 IDENTIFYs an hour: an account that keeps identifying gets flagged,
  so past that ReCore stops reconnecting and says so until it restarts. The first
  heartbeat comes after a random part of the interval, as Discord asks.
- **Typing.** Mention search waits 300 ms after the last key before it asks
  Discord for members, so typing a name sends one search, not one per letter.
- **Cookies.** ReCore keeps the cookies Discord's servers set and sends them
  back where they apply, as any HTTP client with a cookie jar does. These are
  Discord's device cookies and Cloudflare's rate limiting and bot management
  cookies. One jar serves the whole process. Persistent cookies are saved,
  readable only by the user, in `discord-cookies.json` in the data directory,
  so ReCore stays the same device across restarts. ReCore can't pass a Cloudflare
  challenge or captcha, so it never has a `cf_clearance` cookie.
- **Headers.** Every API request carries what the web client's carry:
  `X-Super-Properties`, the same description of the client as the gateway's
  identify, `X-Discord-Locale` and `Accept-Language` for English, and
  `X-Discord-Timezone` with this machine's time zone. `X-Debug-Options`
  stays off, as it does in the web client until a debug option is on.
- **Downloads.** Pictures and files from Discord's CDN download eight at a
  time, over one HTTP/2 connection, as a browser's requests to one host go.
  Avatars, server icons, banners, emoji and stickers have eight turns of
  their own, apart from previews and files, so neither waits behind the
  other. Their newest batch goes first, in its order: rows on screen now
  come before rows scrolled past, and opening the emoji picker queues its
  pictures in the picker's order instead of asking for hundreds at once.
  A start fetches the sticker picker's pictures ahead into the disk cache,
  which decodes none of them and holds none.
  One kept on disk shows without waiting for a turn. A file someone asked
  for, such as the image the viewer shows or one they save or copy, skips
  the queue. A
  download that gets nothing from Discord for 20 seconds gives up, so a
  connection lost to a network change or sleep doesn't hold the queue; while
  requests wait, an HTTP/2 ping that goes unanswered closes that connection.

## Why not an existing bridge

[mautrix-discord](https://github.com/mautrix/discord) bridges messages through
a service hosted next to Synapse. It has no voice, and voice matters most here.
It also adds a Matrix user for every Discord contact, so the homeserver learns
who each person talks to. Beeper uses the same bridge and also has no Discord
calls.

A Discord bot relaying voice into a Matrix room, like
[matrix-discord-voice-bridge](https://github.com/lukacsi/matrix-discord-voice-bridge),
follows Discord's rules. But Discord users would hear every ReCore user as one bot,
it only works in servers that install the bot, and bots can't join DM calls.

## Account linking

Link by QR code, the method mautrix-discord uses. ReCore shows a code from
Discord's remote login service, and the person scans it with the Discord phone
app. A build without Matrix also offers Discord's email and password on its
sign-in screen, with the same warning: `POST /auth/login` with Discord's
payload, then `POST /auth/mfa/totp` with the authenticator code when it asks
for one, and the same captcha page as a QR ticket when it asks for a captcha.
The password is used for the request and dropped; it is never logged or kept.
Discord's real login may still refuse a client that isn't its browser, or
demand a device check its page makes first; and only authenticator and backup
codes are answered, not an SMS code or a passkey. The QR code stays the path
Discord's own clients use. A build with Matrix links from Settings, under
Discord; one without Matrix links on its sign-in screen. Either way, ReCore
first says that Discord doesn't allow third-party clients, so linking puts the
account at risk, and shows the code only when asked.

Settings → Discord lists every account linked on this device, with Add
Account, Switch and Remove. Adding another runs the same QR or token link
without leaving the account in use; when it finishes, that account becomes the
one in use. Switching starts the Discord service again on the chosen saved
token, so no code is scanned and no session is ended: the account left stays
signed in on Discord as this device's session. Removing the account in use
chooses the first one left, and removing the last unlinks ReCore from Discord.

The account's own client settings are read as Discord keeps them in
`PreloadedUserSettings` (`crates/recore-discord/src/settings.rs`): whether
reactions, attachments, embeds and GIFs draw in messages, whether messages
show compactly and emoticons become emoji, the theme, the language, and which
reactions notify. A write of one of those switches carries its whole section,
with the rest as it was: Discord puts each top-level field a PATCH carries in
place of the old one. Voice devices, keybinds and each server's notification
level live outside this message, so they stay Discord's.

HTTPS requests and WebSockets alike trust the certificates the system trusts,
through `rustls-platform-verifier`, so a reverse proxy whose CA is in the
system's store works. The WebSockets share one TLS setup,
`recore_discord::websocket`.

The remote login gateway (`wss://remote-auth-gateway.discord.gg/?v=2`, with
Discord's web origin) works as Discord's own login page does,
`recore_discord::remote_auth`:

1. ReCore makes a 2048-bit RSA key and sends its public half, as base64 DER.
2. Discord sends a nonce encrypted to it (RSA-OAEP with SHA-256), and ReCore
   answers with the SHA-256 of the nonce, base64url, which proves it holds the
   key.
3. Discord sends a fingerprint, and the code carries
   `https://discord.com/ra/{fingerprint}`.
4. Scanning it sends who scanned (`id:discriminator:avatar:username`, encrypted
   to the key), and ReCore shows "Confirm on your phone".
5. Confirming sends a ticket, which `POST /users/@me/remote-auth/login` trades
   for the login, encrypted to the key again.

The gateway closes the connection when the QR session times out, and ReCore
connects again for a new code while the code is on screen. Saying no on
the phone stops linking, with "Try again". The key lives only in memory for the
one attempt. The `rsa` crate carries the Marvin timing advisory
(RUSTSEC-2023-0071), which needs an attacker to time many decryptions; ReCore
decrypts three messages from Discord per attempt. Linking connects Discord at
once, without restarting: voice reaches it through a slot the chat backend
fills. When Discord can't start then, or as ReCore starts, such as when the
credential store can't be read, ReCore says why. What waits on Discord, such as
a friend request, the custom status or a profile, fails with that reason
rather than waiting, as it does when the chat backend stops.

Discord may ask for a captcha when trading the ticket or checking an email
login, as it does of logins from devices it doesn't know ([captcha
handling](https://docs.discord.food/topics/captcha-handling)): a 400 with
`captcha_key`, an hCaptcha site key that changes, and often `captcha_rqdata`,
`captcha_rqtoken` and `captcha_session_id`. ReCore has no web view, so
`captcha.rs` serves a page with the challenge on `127.0.0.1`, at a random path,
and opens it in the browser; the dialog says so and can open it again. The page
loads hCaptcha's `api.js` with `host=discord.com`, as hCaptcha's mobile SDKs
name the host they stand for, sets the rqdata, and posts the solution back.
ReCore trades the ticket again with `X-Captcha-Key`, `X-Captcha-Rqtoken` and
`X-Captcha-Session-Id`. While a challenge is open, ReCore sends heartbeats if
the gateway is still connected. It shows up to three challenges per attempt. The
listener lives only while the page waits, so nothing stays loaded afterwards.
It serves browser requests independently so an idle connection can't delay the
challenge. If Discord rejects the ticket with code 20042, ReCore asks you to
start again with a new QR code. [Joining by invite](servers.md#joining-by-invite)
shows Discord's captchas the same way.

Each account's token lives in the OS credential store under a name derived
from its Discord user id, discovered with `GET /users/@me` after linking, next
to the Matrix session. The token gives full control of the account, so it
never goes into Matrix or a file, not even encrypted. `layout.json` keeps only
the list of ids with their display names and the id in use. A device that
links appears in Discord as its own session.

Switching restarts the Discord service on the chosen token and leaves the
previous session's token valid; the previous account's voice call ends, as
voice belongs to the one account in use. Unlinking one account, from Settings
or by signing out of a build without Matrix, leaves any Discord call, ends
that account's session with `POST /auth/logout`, deletes only its token, and
starts the next saved account when one remains. The last account unlinked
returns a build without Matrix to its sign-in screen.

## Later

In rough order. The browser covers these until then.

- Sorting and filtering a forum's posts by tag, and the gallery layout of
  forums and media channels.
- Discord's voice settings beyond those above, and push-to-talk's sounds.
- Activities, and the rest of a server's settings.
- [Mirroring into Matrix](#mirroring-into-matrix).
- The blocked on the friends page, beside Online and All.
- A custom status's end while ReCore is closed, or set from a device whose
  clock is off: ReCore trusts the time Discord gives.
- Opening embed pictures from a card in the viewer with their neighbours.
- A smaller READY. ReCore identifies with an empty
  `client_state.guild_versions`, so every connect brings every server in full.
  Discord's web client, when it has a local cache, sends each server's
  version from its last READY, with `highest_last_message_id`,
  `read_state_version`, `user_guild_settings_version`,
  `user_settings_version`, `private_channels_version` and `api_code_version`,
  and Discord leaves out what hasn't changed. ReCore would keep those versions
  beside its saved snapshot, fill in what READY leaves out from it, and the
  fake would answer the same way.

## Mirroring into Matrix

Planned, not started: copying Discord DMs and chosen server channels into
Matrix rooms on the person's homeserver, so devices without a Discord login
can read and reply, and history builds up there. DMs would mirror by
default and server channels only when chosen, because large servers would
fill the homeserver with other people's history. One linked device at a
time would run the mirror, and the rooms' names and Discord details would
be encrypted, so the homeserver sees only counts, sizes and timing.

## Memory

The Discord connection and its server state count towards the same targets as
the rest of ReCore: under 250 MiB settled idle and under 400 MiB in a ten-person
call. Large servers carry big member lists, so request members only for the
channel on screen. Measure settled idle and a Discord call with the method in
[measurements](../measurements/README.md).

## Testing

`testing/fake_discord.ts` is a local Discord: the gateway, the parts of the
HTTP API ReCore uses, a voice server that answers the handshake without DAVE,
Go Live signalling, the remote login gateway, and Discord's email login with
its two-factor prompt. Set
`RECORE_DISCORD_TEST_SERVER` to it, and ReCore uses a test token instead of
the linked account, kept in memory. With `RECORE_DISCORD_TEST_UNLINKED=1` too,
ReCore starts unlinked and links against the fake, whose controls scan and
confirm the code. `POST /_control/login` makes `POST /auth/login` answer with
a login, ask for `mfa` or `captcha`, or `fail`, and `GET /_control/logins`
lists the emails and codes tried, never a password. The fake resumes a dropped
gateway session, replaying what it missed, and can send op 7. The tests in `crates/recore-discord/tests/fake_server/`
run against it, each test on a fake of its own; `scripts/test-fake-discord
FILTER` runs one area, such as `forum`. With `--env PATH`, the fake serves an environment's content in
place of its fixture, for demos and screenshots; see
[building](../building.md#networks) and `testing/fake_discord_env.ts`.
`crates/recore-discord/tests/fake_env.rs` runs against
`testing/envs/example.json`. It counts the audio a call sends, which the
ignored `discord_audio` test `a_call_sends_what_the_microphone_hears` checks:
ReCore joins the Lounge, unmutes and beeps into a microphone that hears its
speakers, as a private PulseAudio of null sinks does, named by `PULSE_SERVER`
with `RECORE_TEST_AUDIO=1` set. What the fake can't show,
such as media and DAVE with Discord's servers, needs real Discord. Use a
separate test account, which the owner provides, in a private test server. Run
these tests by hand, never in CI: repeated automated logins are what gets
accounts flagged. Keep the QR-linked test token in the developer's OS
credential store, like any other Discord login.

## Licences

davey is permissive, so closed-source ReCore can use it. GPL clients
such as [Abaddon](https://github.com/uowuo/abaddon) are reference
material only.
