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

Every participant needs Hammer Pro 2.0.0 or newer: version 2.0.0 uses Team Mapping protocol 2 (streamed map transfer, compressed and delta encoded changes) and cannot join sessions of older versions, nor older versions join it.

While the map arrives, the panel shows a progress bar with the number of objects and the bytes received. The map is sent compressed in parts, then validated and turned into a scene in the background: the editor stays responsive during the whole join.

Hammer Pro 2.0 uses version 2 of the Team Mapping protocol: the host and every guest need Hammer Pro 2.0 or newer. A 1.x guest that tries to join a 2.0 host receives a message in English, French and Russian asking to update. A 2.0 guest that reaches a 1.x host, or opens an invitation made by 1.x, gets a translated message saying which side must update.

Choose the same connection mode as the host. In direct mode, also enter the host's `IP:port`; IPv6 addresses use `[address]:port`. The host chooses a UDP listen port, initially 41840. The host panel lists local listen addresses. Those are not necessarily reachable over the Internet: the host may need its public IP and a matching router/firewall port-forwarding rule. Hammer Pro does not change router or firewall rules automatically. Direct mode uses no relay or public endpoint discovery and does not fall back to a relay.

All participants see the session name, participant nicknames and measured connection round-trip latency.

## Participants, colours and activity

Every participant has a colour chosen from their nickname, the same on every PC and again when they reconnect (two nicknames that want the same colour get different ones), shown next to their nickname in the panel and in the **Team Mapping** menu. In the 2D and 3D views, in that colour:

- their camera is drawn as a small camera pyramid pointing where they look, with their nickname and ping;
- the objects they have selected are outlined, with their nickname above the selection;
- the objects they have just changed are boxed for a moment, and their nickname shows **editing** while they work, so you see who is changing what, including sculpting and painting strokes as they happen.

The **Participants** list shows for each participant: the role, the network round trip to the host (ping), the data they send and receive per second, the average time their edits take to be confirmed by the host (edit), and **editing** while they make changes. Your own row shows your values.

Select **Teleport** next to a participant to switch to the shared map, move your editor camera to their position and match their viewing direction. The 2D views are centered there too. **Follow** keeps your 3D camera on theirs (smoothly, as they move) and keeps the 2D views centered on them; moving your camera, or selecting **Follow** again, stops it. A label at the top of the 3D view shows whom you follow. **More** has **Locate** (turn toward their position while keeping your camera location) and **Show their selection** (frame the objects they have selected). These controls only change your own view, never the map.

## Chat and pings

The **Chat** box sends a short message (up to 200 characters) to everyone in the session. Messages appear in the chat of each participant with the author colour; when a participant's panel is closed they also appear as a notification and the **Team Mapping** menu shows the number of unread messages.

**Ping** shows a pulsing marker in your colour with your nickname, for 8 seconds, at the point in the centre of your 3D view. To mark another point, right click in a 2D or 3D view and choose **Ping here for the team**. Every participant sees the marker in the 2D and 3D views; when it is outside the 3D view, an arrow at the border of the view points toward it. Each ping also appears in the chat with a **Go to** button that frames the point.

Messages and pings travel with the presence updates (camera and selection) and use a few hundred bytes. A participant who joins within 20 seconds of a message still receives it.

These views cost almost nothing. The outlines of the other participants' selections and edits are drawn by the graphics card from a list rebuilt only when a selection, an edit or the map changes (0.01 ms for 20 objects, 0.5 ms for 2,048 objects, 1.7 ms for seven participants with 2,048 objects each, on `gmc_op_v2.vmf` and `hokai_d.vmf`), and the cameras, labels and markers of the four views take 0.04 to 0.15 ms per frame. 1.2.4 redrew those outlines through the interface on every frame: 107 to 126 ms per frame (under 10 FPS) as soon as one participant selected 2,048 brushes, 0.75 to 0.95 s with seven. Between two real editors on `gmc_op_v2.vmf`, the 3D view benchmark gave the same frame time with and without the other participant selecting 2,048 brushes and pinging (35.7 against 36.2 ms median over a direct connection, 34.7 against 35.5 ms through the relay, on a busy PC). A chat line or a ping reached the other editor 0.16 to 0.47 s after the command, and a followed camera settled on the new position in 0.75 to 1.4 s (it moves smoothly). The activity values add about 60 bytes to a presence message and change at most every 2 seconds; a chat line with a ping adds about 0.1 KB. With two editors in one process, an edit reached the other editor in 0.9 to 6.7 ms on `gmc_op_v2.vmf` and `hokai_d.vmf` while the other participant selected 2,048 brushes, followed the author camera and received a chat line and a ping at every edit, against 0.5 to 8.4 ms without them, and the longest editor frame stayed at 1.2 ms.

## Editing and conflicts

The host validates and orders changes. The initial map is transferred when joining; subsequent map messages contain changed objects instead of the entire map. Presence updates are limited to ten per second, and unchanged presence is not resent. The panel reports transferred bytes and synchronization status.

## Speed and synchronization

Every kind of edit is synchronized object by object: props and other point entities, brush creation, moves, resizes, rotations, clips and vertex edits, displacement sculpting and painting, textures and UV alignment, entity keyvalues and outputs, groups, visgroups, hiding and showing, deletions, instances, multi object transforms and large pastes. No participant serializes, validates or rebuilds the whole map for an edit: the editor reads which objects its own edit touched, the host checks only the changed objects against an index of the whole map (identifiers, faces, sizes and limits are kept up to date), and the other editors replace only those objects in their scene, so untouched brushes keep their geometry.

- An edit leaves as soon as it is made, and the editors are woken as soon as a change arrives instead of waiting for their next refresh. A remote edit appears on the other screens within one or two frames plus the network time.
- An object that already exists is sent as the lines that changed since its previous version (a sculpt stroke sends the touched rows of the displacement, not the whole brush). Large messages from the host are compressed, and the host encodes each change once for all participants.
- One edit at a time is in flight per participant. Edits made while it travels are combined into the next one, so fast repeated edits never queue up and the final state is always sent.
- **Show my strokes and drags to the others while I edit** (setting `workflow.team_live_edits`, on by default): sculpting, painting and other edits that stay open while the mouse button is held are sent up to 30 times per second while they happen, for strokes touching up to 64 objects. Larger strokes, and every stroke when the option is off, are sent when they end. A whole stroke is one step of your personal Undo.
- Hiding and showing objects (Hide selected, Hide unselected, Show all, visgroup check boxes) is shared like any other edit: point entities, brush entities, world brushes and grouped objects are hidden or shown again for everyone at once, in the 2D and 3D views, and your Undo and Redo bring them back for everyone.
- Changes from the others keep appearing while you drag or sculpt. Only the objects your own operation is editing wait until it ends.
- The **Synchronization** section of the panel shows the revision, the traffic, the round trip of your last edit and its average, the time taken to apply the last remote edit and its maximum, the time taken to capture your last edit, the number of edits sent, received and sent live, and the live edit option.
- The safety snapshots are written by the host network thread and a background task, without pausing the editor.
- Hosting no longer pauses the editor either: the shared copy of the map is prepared a few milliseconds per frame, then validated in the background.

Measured with two editors on one PC (direct connection), 2.0.0 against 1.2.4, on `hokai_d.vmf` (73 MB, 30,550 objects): moving a brush reached the other screen in 4 to 35 ms instead of 56 to 64 seconds, moving a prop in 0.4 to 59 ms instead of up to 0.15 s; the longest editor frame during these edits dropped from 25.8 s to 41 ms; hosting froze the editor 14.3 s and now uses at most 4 ms per frame; joining took 44.5 s with a 9 s freeze and now 9 to 11 s with no frame above 1.1 ms. With four guests, the host spends about 1 ms per edit (13 ms at most) instead of 9 to 14 s. A brush move sends 0.6 KB instead of 2.1 KB, a displacement stroke 0.45 KB instead of 12 KB, a paste of 500 brushes 57 KB instead of 987 KB. Between two real editors through the public relay on `gmc_op_v2.vmf`, a brush move is visible on the other editor 0.3 to 0.5 s after the command (measured through MCP automation, which itself waits for frames), against 15 to 17 s with 1.2.4; joining takes 3.3 s and joining again after leaving 3.4 s.

An object selected by another participant cannot accept your changes. When some of your selected objects are also selected by someone else, the panel says who and how many, and the focused view shows **Also selected by** with the nickname, before you even try to change them. If you change one anyway, the message names the participant and the object, for example `Refused: light 3 is selected by Ana`, and the object is boxed in red for a few seconds. Concurrent changes to different objects can merge. Changes based on an older version of the same object are rejected rather than silently overwriting newer work. A rejected local version is retained in **Recover local copy (Save As)**.

Undo and Redo apply to your own acknowledged changes. If another participant has since changed the affected object, the undo is refused rather than overwriting their change. Wait for the pending edit to synchronize before using Undo. Camera positions and selections update during a drag.

When two participants change the same object at the same time, the host keeps the change it receives first. The other participant sees the kept version, the message names who changed which object first (for example `Conflict: Ana changed brush 120 first`), the objects are boxed in red for a few seconds, and only that participant's version of the conflicting objects goes to **Recover local copy (Save As)**; the other objects of the same edit are sent again automatically. An edit the host refuses for another reason (for example a path outside the content folders, or a script entity when the host turned script sharing off) is replaced by the shared version and kept in the recovery copy too. **Recover local copy** writes the shared map with your versions of those objects, in the background.

## Connection loss and reconnection

When the connection of a guest drops (network failure, Wi-Fi change, sleep, a host that stopped answering, a guest that fell too far behind), Hammer Pro reconnects by itself: the panel shows **Connection lost** with the reason and the next attempt, and tries 6 times over about one minute (after 1, 2, 4, 8, 15 and 30 seconds). **Reconnect now** tries at once, **Stay offline** gives up and keeps the map only on this PC. After the last attempt the panel offers **Try again**.

- The shared map stays open in the same tab, and you can keep working while offline. Undo and Redo work on your local history meanwhile.
- When the connection is back, the map is downloaded again into the same tab (no new tab). Your installation keeps its identifier range, so objects you created offline keep their ids.
- The edits made while offline, and an edit that was still waiting for the host, are sent again automatically when nobody else changed the same objects in the meantime. They form one step of your personal Undo. Objects that someone else changed meanwhile keep the shared version, and your versions go to **Recover local copy (Save As)**.
- A participant removed or banned by the host, a session ended by the host, a changed password or a nickname taken by another installation are not retried.
- **Resync** (in **Synchronization**) does the same on demand while you are connected: it downloads the shared map again and sends your unconfirmed edits again. Use it if your view of the map ever looks different from the others'.
- On the host, a guest reconnecting from the same installation replaces its stale connection at once, so the nickname is not refused as already in use.
- Measured on `gmc_op_v2.vmf` (31 MB) between two real editors: **Resync** took 3.8 s over a direct connection and 2.3 s through the relay.

## Relay and address privacy

**Relay is the default.** In this mode, direct IP transports and direct address publication are disabled, without automatic fallback to a direct peer connection. Fresh network endpoint identities are used for every session connection, so a previous direct endpoint identity is not reused. All participants in a relay session use the relay path.

**Direct connection is an explicit alternative.** It can reduce latency by avoiding an intermediary, but does not hide the host or guest network address from the machine it connects to. It can fail behind restrictive NAT or firewalls. Transport encryption and session authentication remain enabled in both modes. Changing modes requires a new session.

Network captures still show connections between your PC and relay or discovery infrastructure. The other map participants receive no direct network address from the collaboration protocol. The relay operator can observe connection addresses, timing and traffic volume, but cannot decrypt the end-to-end encrypted map traffic.

Hammer Pro is free and open source. Using public third-party relays avoids recurring server hosting costs for the project. The built-in Iroh/n0 relay and discovery infrastructure needs no configuration or paid account in Hammer Pro. Its availability, capacity, future pricing and network reachability are controlled by its operator. Hammer Pro cannot guarantee permanent free service or connectivity through every restrictive network. A relay can add latency; the panel shows measured ping, not a promised delay. No dedicated relay server needs to be operated by the map host or Hammer Pro maintainer.

## Host moderation

The host can remove a participant or ban them for a chosen duration, from the **More** menu of the participant row. A duration of zero means until the session ends. Enter a reason for the refusal message. The session ban list shows nicknames, reasons and remaining durations and lets the host unban participants. Bans expire automatically and are discarded with the session.

Removal alone allows reconnection with valid credentials, unless the session is closed to new participants or asks for approval: a removed participant then has to be admitted again. A ban matches a cryptographically proven installation identity scoped to the current host session. The local installation key is stored in `team-mapping-identity.key` under the Hammer Pro settings directory. Raw hardware serial numbers are not collected. This is not an unforgeable hardware ban: a modified open source client or reset installation identity can bypass it. Relay-only operation deliberately does not disclose guest IPs, so the host cannot reliably enforce bans on their real IP addresses.

**Rotate access and disconnect guests** (in **Host tools**, with the ban list and the safety snapshots) generates a new invitation secret, changes the password and disconnects current guests. Old invitations are revoked. Share the new invitation and password only with participants you want to readmit. Ending the session revokes its invitation as well.

## Access control and roles

Available from Hammer Pro 2.0. Choose these options under **Session security** before hosting, or in **Access control** while the session runs:

- **Ask me before admitting a new participant**: a new participant waits until the host selects **Admit**, **Refuse** or **Ban**. The host gets a notification for every request. A request without an answer is refused after two minutes. Participants already admitted in this session rejoin without asking again.
- **New participants are read only**: they join as viewers. A viewer sees the map, the cameras and the selections, but every edit is refused and their selection never locks objects for the others. The host can switch any guest between read only and editing at any time with the **Read only** box; the role is kept when the guest reconnects.
- **Closed to new participants**: nobody new can join. Participants already admitted can still reconnect after a network interruption.
- **Invitation valid for new participants**: after this many minutes (0 means the whole session), the invitation only readmits participants already admitted. Rotating access starts a new period.

**Remove** disconnects a participant and forgets their installation, so with approval or a closed session they must be admitted again. **Rotate access and disconnect guests** also forgets every admitted installation and resets the wrong password counter.

The panel shows the security counters: wrong passwords, invalid connection attempts, refused joins, participants slowed down for sending too fast and resynchronizations of participants that fell behind. When someone who has the invitation types a wrong password, the host gets a warning: rotate access if it was not one of your guests.

## Security model

- **Encryption**: every connection is QUIC with TLS 1.3 from end to end (rustls), in relay and in direct mode. There is no unencrypted fallback. The invitation pins the public key of the host, so neither a relay nor a fake host can impersonate it.
- **Authentication**: a guest proves that they know the invitation secret and the password with keyed BLAKE3 proofs over a fresh challenge of the host. The proofs are bound to the TLS session itself (key exporter), the host and guest keys, the nickname and the installation key, so they cannot be replayed on another connection, and the password never travels. Checks run in constant time. The installation key signs the proof: nobody can claim another installation to dodge a ban.
- **Password guessing**: after five wrong passwords, password attempts pause for 30 seconds, then 1, 2, 4, 8 and at most 15 minutes. The counter drops by one every five minutes. During a pause, the passwords of installations not yet admitted are not even checked. Participants already admitted in this session reconnect normally during a pause, because their installation key is verified first. Connections without the invitation secret are counted apart and never cause a pause: neither a stranger nor someone who only has the invitation can lock your guests out.
- **Connection floods**: at most 16 connections can be authenticating at the same time, 4 of them from one IP address in direct mode. A connection leaves these limits as soon as it is authenticated, so a whole team behind one router or several Hammer Pro windows on one PC can use every participant slot. A guest that finds the host busy retries by itself a few times, then gets a translated "host busy" message. A handshake that does not finish within 15 seconds is dropped. Banned IP addresses (direct mode) are refused before any password check.
- **Message checks**: both sides check every message: sizes, counts, nesting depth, coordinates (finite, within 1,000,000 units), object keys, identifiers and participant ranges, nicknames, roles, latencies and reasons (at most 256 characters, control characters removed). Revisions must follow each other, and an edit sequence that is replayed or out of order disconnects the participant.
- **Flow control instead of refusals**: the host reads each participant at up to 64 messages and 32 MiB per second on average, with bursts of 128 messages and 64 MiB. A participant that sends faster is slowed down, never disconnected: the host reads its connection more slowly and the network flow control holds the sender. Camera updates above 15 per second and pings above 4 per second are ignored. Edits wait in a queue on the host and are processed one participant after the other, so a participant who sends many edits at once never delays the others, and the host's own edits go first. The host only stops taking edits while its own editor is behind (4,096 waiting events or 256 MiB) and resumes as soon as it catches up. At most 24 MiB of unprocessed messages per participant wait on the host (larger edits one at a time for the whole session), and the memory for a message is reserved in full only after its first MiB has arrived.
- **Slow connections**: each participant has its own send queue on the host, so a participant whose map is still downloading, or whose connection is slow, receives every change made meanwhile once it catches up. A participant that falls more than 256 MiB of changes behind receives the current map instead of the waiting changes, with the result of its own pending edits: nobody is disconnected for being slow. The panel counts these resynchronizations.
- **Refused edits**: an edit is never refused because the host or the network is busy. When an edit breaks a rule (an object selected by someone else, an identifier outside your range, a read only participant, a value the map rules refuse, an edit above the session size limits), the participant stays connected, sees the reason in their own language with the object concerned, and keeps their version in **Recover local copy**. A refused edit no longer sends the whole map back.
- **Bounded resources**: at most 256 installations per invitation. An installation that reconnects keeps its identifier range, two windows of the same installation in one session get separate ranges, and a stale connection of the same installation with the same nickname is replaced instead of blocking that nickname.
- **Files**: the protocol transfers no file. Resource paths are shared as Hammer, Hammer++ and the engine write them (either separator, relative instance files); only references that leave the content folders are refused, such as drive letters and network paths. A participant never opens a path chosen by another participant outside its own map, game and custom content folders: Hammer++ background images with a network, device or outside path are not loaded during a session, and instance files are resolved from the participant's own folders. Script and command entities are shared by default, because they only run in the game, never in the editor; the host can turn this off with **Share script and command entities** in the session security options (setting `workflow.team_share_scripts`).
- **Cost**: the checks add about one microsecond to a camera update and well under a millisecond to an edit on the host. A refused edit now sends about a hundred bytes back instead of the whole map (84 MB on a 73 MB map, which took seconds), each change is serialized once for all guests, and brush edits on large maps are accepted 1.5 to 3 times faster than in 1.2.4 because only the changed brushes are parsed again. A participant who sends many edits at once no longer delays the others: while one participant created 12 brushes in a row, another participant's prop move was confirmed in 0.1 s instead of 7.9 s with 1.2.4 on `gm_construct.vmf`, and in 2.5 s (the time of the brush being checked) instead of 68 s on `hokai_d.vmf`. The host's own edit waited 0.1 s instead of 4.5 s and 2.5 s instead of 38 s, and no camera update of the host was lost (1.2.4 dropped 593 of 635 on `hokai_d.vmf`). With 15 guests, an edit reaches every guest as fast as with 7 (about 55 ms with every participant on one PC).
- **MCP**: `get_team_session`, `set_team_access` and `moderate_team_session` give the session state, the access policy and the moderation actions to an AI. They never return the invitation or the password.

## Safety snapshots

The host receives a notification when a nickname joins. Compressed local snapshots are saved when hosting begins and every ten minutes. **Save snapshot now** adds a manual copy. The snapshot folder keeps at most ten files across sessions, deleting the oldest only after a new file has been written successfully. The list shows the session name, UTC date, revision and actual compressed size. Compression depends on map contents.

Select a snapshot and confirm **Restore** to replace the shared document for all participants. The host first saves the current authoritative map. If that safety backup fails, restoration is cancelled. Session Undo/Redo histories are reset; unconfirmed local work is kept in the recovery copy when needed.

Snapshots are stored beside ordinary autosaves in `HammerPro/team-mapping-snapshots` under local application data and remain after the session ends. Guests cannot read or restore the host's snapshots. Nothing is uploaded to cloud storage. Disk errors are shown in the panel; storage failure can still prevent recovery.

## Compilation, saving and resources

Compilation runs only on the PC whose user requests it. Its compiler configuration, build directory, BSP, logs, copy into `garrysmod/maps` and game launch stay local. Compilation takes a local snapshot; later session changes are not inserted into a running compile. A guest cannot instruct the host to compile, run a shell command, launch a program or write an output file.

Textures, models, scripts and mounted content are not automatically downloaded or installed through a session. Each participant needs their own content and compiler installation. Missing resources can produce a different preview or a failed compile. The host's local compiler commands and filesystem paths are not session settings.

## Security boundaries and current limits

- Up to sixteen participants including the host. The host uploads accepted changes to each guest, so bandwidth grows with the number of guests and the amount of edited geometry.
- The shared VMF is limited to 500 MiB of session data and 200,000 object units. An edit can affect at most 8,192 units and use at most 8 MiB of serialized change data.
- Incoming frames, nesting, property lengths, coordinates, duplicate IDs and per-peer traffic are bounded. The network protocol exposes map changes and presence, not the editor MCP API or arbitrary file operations.
- Resource paths (models, materials, textures, sky names, instance files, sounds) are shared as Hammer, Hammer++ and the engine write them: backslash or slash separators, upper or lower case, `..` inside the path. A leading slash means the root of the content folders, as in the engine. Only references that leave the content folders are refused: drive letters, network paths, paths that climb above their folder, and absolute instance file paths (an instance file may climb at most two folders above the map folder, as `../instances/` layouts do; every participant resolves it from its own map or game folder). Script and command entities such as `lua_run`, `logic_script`, `point_servercommand` and `point_clientcommand`, and outputs that call `Command`, `RunScriptCode`, `RunScriptFile` or `RunPassedCode`, are shared by default: they only run in the game, never in the editor, so a map like `gm_construct.vmf` with its `lua_run` entities can be hosted as it is. When another participant adds or changes one, the others get a notice naming the participant and the entity (for example `Ana added or changed a script or command entity: lua_run (entity:120)`). The host can untick **Share script and command entities** (setting `workflow.team_share_scripts`, MCP `set_team_access` with `share_scripts`): its own script entities and inputs then stay on its PC, and the ones the others add or change are refused with a message naming the object, for example `entity:120: class "lua_run" refused: script and command entities cannot be shared`. Turning it back on shares the host's own ones at once.
- A refused value never blocks the session: the entity, the key and the value are named in a notification and in the panel list **values kept only on this PC**, the value stays in your own copy of the map, and the rest of the object is shared. When the host turned script sharing off, a refused script entity stays entirely on your PC. An edit received from another participant with such a value is refused with the same precise message.
- Local save paths, view settings, camera bookmarks and compile settings are not shared.
- Only the chosen map is shared. Other open maps remain local, and synchronization continues while you use another tab. **Return to shared map** reopens the session document. Closing the shared map tab ends the session. Each participant keeps their local document after disconnection.
- There is no host migration or persistent cloud room: when the host leaves or its editor stops, the session ends for everyone and each participant keeps the map. Guests reconnect by themselves after a network interruption (see Connection loss and reconnection). Check synchronization before compiling or saving a final version.
- Server validation protects the shared document from malformed or stale messages. It does not make an invited editor trustworthy: someone allowed to edit the map can still propose unwanted map changes. Use moderation and backups.

## Automation (MCP)

The MCP server of the window exposes the session: `get_team_mapping_status` (connection, role, invitation for the host, revision, pending edit, join progress, traffic, synchronization timings, edit counts, recovery copy size, participants with their colour, ping, traffic, edit time, activity and shared selections, the participant you follow, the recent chat and pings, the reconnection state, the resynchronizations and the last script notices), `team_mapping_host`, `team_mapping_join`, `team_mapping_chat` (send a message), `team_mapping_ping` (show a marker at a point, or at the centre of the 3D view), `team_mapping_camera` (teleport, look, follow, show their selection, stop following), `team_mapping_reconnect` (reconnect now, resync, or `stop` to stay offline), `team_mapping_save_recovery` (writes the recovery copy) and `team_mapping_leave`. The status also lists the values kept only on this PC. The live edit option is the setting `workflow.team_live_edits` (`set_setting`). See [MCP](../../MCP.md).

See also [game setup](../getting-started/game-setup.md), [compilation](../compiler/compile-window.md) and [VMF compatibility](../getting-started/vmf-compatibility.md).

## Connection troubleshooting in 1.2.2

Update the host and every guest to 1.2.2. The fixes for valid Hammer++ vertex loops, repeated visgroup memberships and scalar moving-brush `startposition` settings remain included. A genuinely malformed map is still rejected with its property and block name.

Presence, ping and editor statistics now retain their latest values while a map is loading, instead of filling the queue for ordered map changes. A guest downloading slowly is not disconnected merely because camera positions changed during the initial transfer. Transfer timeouts measure stalled progress rather than the total duration of an actively progressing message. After the initial map is sent, the guest has up to five minutes to validate it and begin sending session messages.

A relay session is not advertised as ready until a relay is reachable. If a host cannot reach one, creation reports a relay error. Connection errors retain their underlying cause instead of showing only `connection lost`. Relays still depend on the operator and local network access; this does not bypass a blocked network or add automatic reconnection.
