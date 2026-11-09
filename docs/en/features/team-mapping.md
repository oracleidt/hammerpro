[Documentation](../README.md) > Team Mapping | [Français](../../fr/features/team-mapping.md) | [Русский](../../ru/features/team-mapping.md)

# Team Mapping

Team Mapping shares a VMF between an editor hosting a session and its invited participants. Open **Team Mapping** in the top menu, immediately before **MCP**.

## Create a session

1. Open the VMF you want to share.
2. Choose **Relay (recommended)** for private addresses and automatic connection, or **Direct connection** if you can provide a reachable host IP and UDP port.
3. Enter a session name using 1 to 48 ASCII letters, digits, spaces, underscores or hyphens.
4. Choose a nickname using 1 to 12 ASCII letters, digits, underscores or hyphens. Nicknames must be unique within the session, ignoring case.
5. Enter a password of 8 to 256 characters. Passwords are masked; the visibility control is explicit.
6. Choose the map in **Map to share**, then select **Host selected map**, wait for the connection status, then select **Copy invitation**.
7. Give participants the invitation, session name and password through a trusted channel.

The invitation is a credential, not a public room listing. It contains a random session access secret and pins the host's cryptographic identity. Do not publish it. The password is also required. Relay mode needs no account, manual IP address, port forwarding, router changes or separately installed server.

## Join a session

Enter the invitation, exact session name, your nickname and the password. Select **Join in a new local tab**. Your previously open document is kept in its own tab. The shared map has no inherited host filesystem path. Save your own copy with **Save As** before compiling if requested.

Choose the same connection mode as the host. In direct mode, also enter the host's `IP:port`; IPv6 addresses use `[address]:port`. The host chooses a UDP listen port, initially 41840. The host panel lists local listen addresses. Those are not necessarily reachable over the Internet: the host may need its public IP and a matching router/firewall port-forwarding rule. Hammer Pro does not change router or firewall rules automatically. Direct mode uses no relay or public endpoint discovery and does not fall back to a relay.

All participants see the session name, participant nicknames and measured connection round-trip latency. Other participants' camera positions appear as colored points with a direction marker. Their selected objects are outlined in the same color. The Participants panel can point your camera toward a participant.

## Editing and conflicts

The host validates and orders changes. The initial map is transferred when joining; subsequent map messages contain changed objects instead of the entire map. Presence updates are limited to ten per second, and unchanged presence is not resent. The panel reports transferred bytes and synchronization status.

An object selected by another participant cannot accept your changes. Concurrent changes to different objects can merge. Changes based on an older version of the same object are rejected rather than silently overwriting newer work. A rejected local version is retained in **Recover local copy (Save As)**.

Undo and Redo apply to your own acknowledged changes. If another participant has since changed the affected object, the undo is refused rather than overwriting their change. Wait for the pending edit to synchronize before using Undo. Camera positions and selections update during a drag. Geometry edits are sent when the tool commits its transaction. Incoming changes continue to synchronize in the background and merge into the visible document when the local operation ends.

## Relay and address privacy

**Relay is the default.** In this mode, direct IP transports and direct address publication are disabled, without automatic fallback to a direct peer connection. Fresh network endpoint identities are used for every session connection, so a previous direct endpoint identity is not reused. All participants in a relay session use the relay path.

**Direct connection is an explicit alternative.** It can reduce latency by avoiding an intermediary, but does not hide the host or guest network address from the machine it connects to. It can fail behind restrictive NAT or firewalls. Transport encryption and session authentication remain enabled in both modes. Changing modes requires a new session.

Network captures still show connections between your PC and relay or discovery infrastructure. The other map participants receive no direct network address from the collaboration protocol. The relay operator can observe connection addresses, timing and traffic volume, but cannot decrypt the end-to-end encrypted map traffic.

Hammer Pro is free and open source. Using public third-party relays avoids recurring server hosting costs for the project. The built-in Iroh/n0 relay and discovery infrastructure needs no configuration or paid account in Hammer Pro. Its availability, capacity, future pricing and network reachability are controlled by its operator. Hammer Pro cannot guarantee permanent free service or connectivity through every restrictive network. A relay can add latency; the panel shows measured ping, not a promised delay. No dedicated relay server needs to be operated by the map host or Hammer Pro maintainer.

## Host moderation

The host can remove a participant or ban them for a chosen duration. A duration of zero means until the session ends. Enter a reason for the refusal message. The session ban list shows nicknames, reasons and remaining durations and lets the host unban participants. Bans expire automatically and are discarded with the session.

Removal alone allows reconnection with valid credentials. A ban matches a cryptographically proven installation identity scoped to the current host session. The local installation key is stored in `team-mapping-identity.key` under the Hammer Pro settings directory. Raw hardware serial numbers are not collected. This is not an unforgeable hardware ban: a modified open source client or reset installation identity can bypass it. Relay-only operation deliberately does not disclose guest IPs, so the host cannot reliably enforce bans on their real IP addresses.

**Rotate access and disconnect guests** generates a new invitation secret, changes the password and disconnects current guests. Old invitations are revoked. Share the new invitation and password only with participants you want to readmit. Ending the session revokes its invitation as well.

## Safety snapshots

The host receives a notification when a nickname joins. Compressed local snapshots are saved when hosting begins and every ten minutes. **Save snapshot now** adds a manual copy. The snapshot folder keeps at most ten files across sessions, deleting the oldest only after a new file has been written successfully. The list shows the session name, UTC date, revision and actual compressed size. Compression depends on map contents.

Select a snapshot and confirm **Restore** to replace the shared document for all participants. The host first saves the current authoritative map. If that safety backup fails, restoration is cancelled. Session Undo/Redo histories are reset; unconfirmed local work is kept in the recovery copy when needed.

Snapshots are stored beside ordinary autosaves in `HammerPro/team-mapping-snapshots` under local application data and remain after the session ends. Guests cannot read or restore the host's snapshots. Nothing is uploaded to cloud storage. Disk errors are shown in the panel; storage failure can still prevent recovery.

## Compilation, saving and resources

Compilation runs only on the PC whose user requests it. Its compiler configuration, build directory, BSP, logs, copy into `garrysmod/maps` and game launch stay local. Compilation takes a local snapshot; later session changes are not inserted into a running compile. A guest cannot instruct the host to compile, run a shell command, launch a program or write an output file.

Textures, models, scripts and mounted content are not automatically downloaded or installed through a session. Each participant needs their own content and compiler installation. Missing resources can produce a different preview or a failed compile. The host's local compiler commands and filesystem paths are not session settings.

## Security boundaries and current limits

- Up to eight participants including the host. The host uploads accepted changes to each guest, so bandwidth grows with the number of guests and the amount of edited geometry.
- The shared VMF is limited to 64 MiB of session data and 200,000 object units. An edit can affect at most 8,192 units and use at most 8 MiB of serialized change data.
- Incoming frames, nesting, property lengths, coordinates, duplicate IDs and per-peer traffic are bounded. The network protocol exposes map changes and presence, not the editor MCP API or arbitrary file operations.
- Unsafe resource paths and script/command entities such as `lua_run`, `logic_script`, `point_servercommand` and `point_clientcommand` cannot be shared. Maps using these features must be adapted before starting a session.
- Local save paths, view settings, camera bookmarks and compile settings are not shared.
- Only the chosen map is shared. Other open maps remain local, and synchronization continues while you use another tab. **Return to shared map** reopens the session document. Closing the shared map tab ends the session. Each participant keeps their local document after disconnection.
- There is no host migration, offline edit merge, automatic reconnect or persistent cloud room. After a network interruption, rejoin with valid credentials. Check synchronization before compiling or saving a final version.
- Server validation protects the shared document from malformed or stale messages. It does not make an invited editor trustworthy: someone allowed to edit the map can still propose unwanted map changes. Use moderation and backups.

See also [game setup](../getting-started/game-setup.md), [compilation](../compiler/compile-window.md) and [VMF compatibility](../getting-started/vmf-compatibility.md).
