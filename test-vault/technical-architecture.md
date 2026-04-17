# Technical Architecture

## System Overview

The proposed system is a secure, offline-first messaging and collaboration platform designed for naval operations. It runs entirely on the Navy's own infrastructure (fiber, satellite, radio) with zero internet dependency. The architecture is designed to function seamlessly across four fundamentally different network conditions that a naval force encounters:

| Environment | Transport | Bandwidth | Latency | Connectivity |
|-------------|-----------|-----------|---------|--------------|
| Shore (base, headquarters) | Fiber optic LAN | Gigabit | < 10 ms | Always-on, full duplex |
| Ship to shore | SATCOM (VSAT, GSAT-7R) | 256 kbps to 2 Mbps shared | 600 ms+ | Intermittent, scheduled windows |
| Ship to ship | SDR radio (HF/UHF/VHF) | 9.6 kbps to 64 kbps | Variable | Opportunistic, range-dependent |
| Submarine | VLF (Very Low Frequency) | ~400 bps | Minutes to hours | Receive-only while submerged |

The system must work in all four conditions without requiring the user to think about which condition they are in. The application layer is identical regardless of transport; only the sync speed and available features change.

---

## Three-Layer Architecture

The system is organized into three distinct layers, each with a clear responsibility boundary:

```
+--------------------------------------------------+
|  Layer 3: APPLICATION                             |
|  Channels, DMs, Files, Search, Notifications      |
+--------------------------------------------------+
|  Layer 2: SYNC ENGINE                             |
|  CRDTs, HLC Ordering, Delta Sync, Priority Queue  |
+--------------------------------------------------+
|  Layer 1: TRANSPORT ADAPTERS                      |
|  Fiber/WebSocket | SATCOM/TCP | SDR/Custom | VLF  |
+--------------------------------------------------+
```

**Layer 1: Transport Adapters** handle the physical/logical connection to each network type. They expose a uniform interface to Layer 2: `send(bytes)` and `on_receive(bytes)`. All transport-specific logic (framing, error correction, bandwidth management) is encapsulated within the adapter.

**Layer 2: Sync Engine** is the core of the system. It maintains the local message database, tracks what each peer has and has not seen, computes deltas, and orchestrates sync sessions. It is completely transport-agnostic; it neither knows nor cares whether bytes are flowing over fiber, satellite, radio, or carrier pigeon.

**Layer 3: Application** provides the user-facing functionality: channels, direct messages, file sharing, search, notifications, user management, and administrative tools. It reads from and writes to the local database maintained by the Sync Engine.

This separation ensures that adding a new transport (e.g., a future laser communication link) requires only writing a new Layer 1 adapter, with zero changes to Layers 2 or 3.

---

## Transport Adapter Specifications

### Adapter 1: Fiber (Shore LAN)

| Parameter | Specification |
|-----------|--------------|
| Protocol | TCP with WebSocket upgrade |
| Duplex | Full duplex, persistent connection |
| Bandwidth | Gigabit (effectively unlimited for messaging) |
| Latency | < 10 ms |
| Availability | Always-on |
| Sync mode | Real-time streaming; messages delivered within milliseconds |
| Error handling | TCP guarantees delivery and ordering |
| Authentication | mTLS (mutual TLS) with device certificates |

The fiber adapter is the simplest. It establishes a persistent WebSocket connection to known peers (other servers on the same LAN or connected via WAN) and streams messages in real time. When a message is created locally, it is transmitted to all connected peers within milliseconds. This adapter provides the experience closest to commercial messaging applications like Slack or Teams.

### Adapter 2: SATCOM

| Parameter | Specification |
|-----------|--------------|
| Protocol | TCP over satellite link (VSAT, Ku/Ka band) |
| Bandwidth | 256 kbps to 2 Mbps shared across all ship systems |
| Latency | 600 ms+ (geostationary orbit round-trip) |
| Availability | Intermittent; may have scheduled sync windows |
| Sync mode | Batch delta sync with priority queue |
| Bandwidth allocation | Configurable cap (e.g., 25% of link for messaging) |
| Compression | Mandatory; zstd compression on all payloads |
| Priority handling | P0 (FLASH) messages preempt all other traffic; P6 (read receipts) deferred to low-traffic windows |

The SATCOM adapter is bandwidth-constrained and latency-sensitive. It operates in batch mode: accumulating outbound messages and syncing them in priority order during available windows. The adapter maintains a priority queue with seven levels:

| Priority | Label | Max Delay | Examples |
|----------|-------|-----------|---------|
| P0 | FLASH | < 2 seconds | Nuclear command, imminent threat |
| P1 | IMMEDIATE | < 30 seconds | Contact reports, damage reports |
| P2 | PRIORITY | < 5 minutes | Operational orders, course changes |
| P3 | ROUTINE | < 30 minutes | Watch bills, logistics requests |
| P4 | DEFERRED | < 4 hours | Training schedules, administrative notices |
| P5 | BULK | Next scheduled window | File transfers, archive sync |
| P6 | BACKGROUND | Best effort | Read receipts, presence updates, search index sync |

When bandwidth is scarce, only P0 through P2 messages are transmitted. P3 and below accumulate in the outbound queue and sync when bandwidth permits.

The adapter implements a bandwidth cap to prevent messaging from consuming the entire satellite link (which is shared with other ship systems: navigation updates, weather data, video teleconference, etc.). The cap is configurable by the ship's communication officer.

### Adapter 3: SDR Radio (Ship-to-Ship)

| Parameter | Specification |
|-----------|--------------|
| Protocol | Custom binary protocol over Software Defined Radio |
| Frequencies | HF (3-30 MHz), VHF (30-300 MHz), UHF (300 MHz-3 GHz) |
| Bandwidth | 9.6 kbps to 64 kbps depending on frequency and conditions |
| Latency | Variable; milliseconds (VHF line-of-sight) to seconds (HF ionospheric bounce) |
| Range | VHF/UHF: 20-50 nautical miles (line of sight); HF: hundreds of nautical miles (ionospheric propagation) |
| Error handling | Forward Error Correction (Reed-Solomon coding) |
| Fragmentation | Messages fragmented into fixed-size packets (256 bytes) |
| Mesh capability | Multi-hop relay; Ship A to Ship B to Ship C |
| Discovery | Beacon-based; periodic broadcast on known frequency |

The SDR adapter is the most complex. Radio links are unreliable, low-bandwidth, and half-duplex. The adapter must handle:

**Packet Fragmentation**: Messages larger than the Maximum Transmission Unit (MTU) of 256 bytes are fragmented into numbered packets. Each packet includes a sequence number and total packet count, enabling reassembly at the receiving end. Missing packets are requested via selective negative acknowledgment (SNACK).

**Forward Error Correction (FEC)**: Each packet is encoded with Reed-Solomon error correction codes, adding approximately 20% overhead but enabling recovery of corrupted data without retransmission. For a 256-byte payload, approximately 50 bytes of FEC data are appended, yielding a 306-byte radio frame.

Reed-Solomon encoding: for a message of $k$ data symbols, $2t$ parity symbols are appended, where $t$ is the number of symbol errors that can be corrected. The code is denoted $RS(n, k)$ where $n = k + 2t$. For this system: $RS(306, 256)$ with $t = 25$, correcting up to 25 symbol errors per frame.

**Mesh Relay and Multi-Hop Routing**: When Ship A can reach Ship B but not Ship C, and Ship B can reach Ship C, Ship B acts as a relay. The routing protocol uses a variation of spray-and-wait (see Gossip Protocol section below) to propagate messages through the fleet without flooding.

**Beacon-Based Discovery**: Every $N$ seconds (configurable; default 30 seconds), each ship broadcasts a beacon packet on a predefined frequency. The beacon contains:

```
{
  node_id: 8 bytes,
  node_type: 1 byte (shore/ship/submarine),
  sync_state_hash: 16 bytes (Merkle root of local message database),
  timestamp: 8 bytes,
  position: 8 bytes (optional, lat/lon compressed),
  priority_pending: 1 byte (highest priority unsent message)
}
```

Total beacon size: approximately 50 bytes. When a node hears a beacon, it compares the `sync_state_hash` with its own. If they differ, a sync session is initiated.

### Adapter 4: VLF (Submarine)

| Parameter | Specification |
|-----------|--------------|
| Protocol | Receive-only broadcast |
| Frequency | 3-30 kHz (Very Low Frequency) |
| Bandwidth | ~400 bps |
| Effective throughput | $\frac{400}{8} \approx 50$ bytes/second |
| Latency | Minutes to hours (broadcast schedule) |
| Directionality | Shore to submarine only (while submerged) |
| Classification | FLASH and IMMEDIATE only |
| Reply mechanism | HF or SATCOM when surfaced/at periscope depth |

VLF is the most constrained transport. It is receive-only (submarines cannot transmit on VLF while submerged without revealing their position), extremely low bandwidth, and unidirectional. The adapter is correspondingly simple:

- Shore VLF transmitter broadcasts messages on a schedule
- Submarine receives and decodes
- Only P0 (FLASH) and P1 (IMMEDIATE) messages are transmitted via VLF
- Messages are compressed to absolute minimum (typically < 140 characters)
- When the submarine surfaces or comes to periscope depth, it can reply via HF radio or SATCOM, using those respective adapters

At 50 bytes per second, a 140-character message (approximately 140 bytes after minimal overhead) takes approximately 3 seconds to transmit. A longer message of 500 bytes takes approximately 10 seconds. These constraints mean VLF is reserved for genuinely urgent communication.

---

## Sync Engine (The Core)

The Sync Engine is the heart of the system. It solves the fundamental problem: how do multiple nodes, each with their own local copy of the message database, stay synchronized when they can only communicate intermittently, over varying transports, with no guarantee of connectivity?

### CRDT Foundation

The Sync Engine uses Conflict-free Replicated Data Types (CRDTs) to ensure that all nodes eventually converge to the same state without requiring consensus, coordination, or a central server.

**Why CRDTs?**

In a traditional client-server architecture (like Slack or Teams), a central server is the source of truth. All clients send messages to the server, and the server distributes them. This works when all clients can reach the server. It fails completely when connectivity is intermittent or absent.

CRDTs solve this by ensuring that:
1. Every node can create and store messages locally, without contacting any other node
2. When two nodes connect, they exchange messages that the other is missing
3. The merge operation is commutative, associative, and idempotent, meaning the order in which messages are received does not affect the final state

Formally, a CRDT guarantees Strong Eventual Consistency (SEC): if two nodes have received the same set of updates (in any order), they are in the same state.

**G-Set (Grow-Only Set) for Messages**

The message store is modeled as a G-Set: a set that only grows (messages are added but never deleted). This is appropriate because:
- Messages in a military context should never be deleted (audit trail requirement)
- A grow-only set is the simplest CRDT; merge is simply set union
- Merge operation: $S_1 \cup S_2$ (union of message sets from two nodes)
- This operation is commutative ($S_1 \cup S_2 = S_2 \cup S_1$), associative ($(S_1 \cup S_2) \cup S_3 = S_1 \cup (S_2 \cup S_3)$), and idempotent ($S_1 \cup S_1 = S_1$)

In practice, the G-Set is implemented as an append-only log per node. Each node maintains its own log of messages it has created, and sync consists of exchanging log entries.

### Hybrid Logical Clocks (HLC)

Messages need a total ordering for display purposes (users expect to see messages in chronological order). Physical wall clocks are insufficient because:
- Clocks on different ships drift relative to each other
- NTP synchronization is unavailable when disconnected
- Two messages created at the "same" wall clock time on different ships need a tiebreaker

Hybrid Logical Clocks (HLC), as defined by Kulkarni et al. (2014), combine physical timestamps with logical counters:

$$HLC = (wall\_clock, logical\_counter, node\_id)$$

The algorithm:
1. When creating a message: $HLC.wall\_clock = \max(local\_clock, last\_HLC.wall\_clock)$. If $HLC.wall\_clock = last\_HLC.wall\_clock$, increment $logical\_counter$; otherwise, reset $logical\_counter = 0$.
2. When receiving a message: $HLC.wall\_clock = \max(local\_clock, received\_HLC.wall\_clock, last\_HLC.wall\_clock)$. Adjust $logical\_counter$ accordingly.
3. Tiebreaker: if two HLCs have identical $wall\_clock$ and $logical\_counter$, the $node\_id$ breaks the tie deterministically.

This produces a total order that closely tracks real time when clocks are synchronized and remains consistent (though with possible ordering artifacts) when clocks drift.

### Vector Clocks for Sync State

Each node maintains a vector clock that tracks, for every known peer, the latest message it has received from that peer:

$$VC_{node\_A} = \{B: 147, C: 89, D: 203, ...\}$$

This means node A has received messages up to sequence number 147 from node B, up to 89 from C, and up to 203 from D.

When two nodes connect for sync, they exchange vector clocks. Each node can then compute exactly which messages the other is missing:

$$\text{Missing at B from A} = \{m \in A.log \mid m.seq > VC_B[A]\}$$

This enables efficient delta sync: only the missing messages are transmitted, not the entire database.

### Merkle Trees for Efficient Delta Identification

For large message databases, comparing vector clocks alone may be insufficient (if there are thousands of channels and millions of messages). Merkle trees provide a logarithmic-time method for identifying differences:

1. The message database is organized as a binary tree
2. Each leaf node is the hash of a message
3. Each internal node is the hash of its children: $H_{parent} = SHA256(H_{left} \| H_{right})$
4. The root hash summarizes the entire database in 32 bytes

When two nodes connect, they compare root hashes. If identical, no sync is needed. If different, they recurse down the tree, comparing child hashes to identify exactly which subtrees contain differences. This narrows the delta to the specific messages that need to be exchanged, with $O(\log n)$ comparisons for a database of $n$ messages.

### Delta Sync Protocol

A sync session between two nodes proceeds as follows:

1. **Beacon/Connect**: nodes discover each other (via beacon on radio, or persistent connection on fiber/SATCOM)
2. **Exchange sync state**: each node sends its Merkle root hash and vector clock
3. **Compute delta**: each node identifies messages the peer is missing
4. **Priority sort**: missing messages are sorted by priority (P0 first, P6 last)
5. **Transmit delta**: messages are transmitted in priority order
6. **Acknowledge**: receiving node acknowledges receipt; sender updates its record of peer's sync state
7. **Repeat**: if new messages were created during sync, another delta round occurs

On fiber (always-on), steps 1 through 7 happen continuously in real time. On SATCOM, sync sessions occur during scheduled windows. On radio, sync sessions occur opportunistically when ships are in range.

### Priority-Based Sync Queue

Not all messages are equally urgent. The sync engine maintains a priority queue that ensures the most critical messages are synced first:

| Priority | Label | Content Type | Sync Behavior |
|----------|-------|--------------|---------------|
| P0 | FLASH | Nuclear command, imminent threat warning | Immediate transmission on any available transport; interrupts ongoing transfers |
| P1 | IMMEDIATE | Contact reports, damage reports, safety of navigation | Next available transmission slot; preempts P2 and below |
| P2 | PRIORITY | Operational orders, course changes, readiness reports | Transmitted within current sync session |
| P3 | ROUTINE | Watch bills, logistics requests, maintenance reports | Transmitted when P0-P2 queue is empty |
| P4 | DEFERRED | Training schedules, administrative notices, policy updates | Transmitted during low-traffic windows |
| P5 | BULK | File attachments, photos, documents | Transmitted during scheduled bulk transfer windows |
| P6 | BACKGROUND | Read receipts, typing indicators, presence, search index | Best-effort; may be dropped if bandwidth is scarce |

### Compression

All payloads are compressed using zstd (Zstandard) before transmission. Compression ratios for typical message content:

| Content Type | Typical Size (raw) | Compressed Size | Ratio |
|-------------|--------------------|-----------------|----|
| Text message (200 chars) | 200 bytes | 60-80 bytes | 60-70% reduction |
| JSON metadata | 500 bytes | 100-150 bytes | 70-80% reduction |
| Structured data (watch bill) | 5 KB | 1-1.5 KB | 70-80% reduction |
| Pre-compressed image | 50 KB | 49-50 KB | ~0% (already compressed) |

Zstd was selected over alternatives (gzip, brotli, lz4) for its combination of high compression ratio, fast decompression speed, and support for dictionary-based compression (pre-trained dictionaries for military message formats can improve compression by an additional 10-20%).

---

## Message Format

Every message in the system conforms to the following structure:

```json
{
  "id": "UUID-v7 (time-sortable)",
  "prev_hash": "SHA-256 hash of the previous message in this channel (hash chain)",
  "channel": "channel_id (UUID)",
  "author": "user_id (UUID)",
  "hlc": {
    "wall_clock": 1711234567890,
    "counter": 0,
    "node_id": "ship-ins-vikrant"
  },
  "classification": "UNCLASS | CONFIDENTIAL | SECRET",
  "priority": "FLASH | IMMEDIATE | PRIORITY | ROUTINE",
  "content": "encrypted_blob (E2EE ciphertext)",
  "attachments": [
    {
      "id": "attachment_id",
      "name": "damage_report.pdf",
      "size": 245000,
      "hash": "SHA-256 of plaintext",
      "encrypted_blob": "..."
    }
  ],
  "signature": "Ed25519 digital signature over all fields above"
}
```

**Field explanations:**

- **id (UUID-v7)**: UUID version 7 embeds a Unix timestamp in the first 48 bits, making IDs time-sortable while remaining globally unique. This eliminates the need for a central ID-issuing authority.

- **prev_hash**: the SHA-256 hash of the previous message in the same channel, creating a hash chain (similar to a blockchain but without consensus). This provides tamper detection: if any message in the chain is modified, all subsequent hashes become invalid.

- **hlc**: the Hybrid Logical Clock value at message creation time, used for total ordering across nodes.

- **classification**: determines which users can decrypt and view the message. Access control is enforced cryptographically: users without the appropriate key material cannot decrypt messages above their clearance level.

- **priority**: determines sync order and transport selection (see Priority-Based Sync Queue above).

- **content**: the message body, encrypted with the channel's encryption key. Only members of the channel possess the key. Relay nodes, gateways, and transport adapters see only the encrypted blob.

- **signature**: an Ed25519 digital signature computed over all other fields. This provides non-repudiation (the author cannot deny having sent the message) and integrity (any modification to any field invalidates the signature).

---

## Peer Discovery

Different network environments require different discovery mechanisms:

| Environment | Discovery Method | Rationale |
|-------------|-----------------|-----------|
| Shore | Static configuration | Bases and headquarters do not move; server addresses are known in advance |
| Ship to shore | Hardcoded shore gateway address | The shore gateway's SATCOM address is configured during ship commissioning/deployment preparation |
| Ship to ship | Automatic beacon discovery | Ships move; their relative positions change; beacons enable dynamic discovery of neighbors |
| Submarine | Shore-initiated VLF broadcast | Submarines cannot transmit without revealing position; shore initiates all communication |

### Beacon Discovery Protocol (Ship-to-Ship)

Ships broadcast a discovery beacon every $N$ seconds (default: 30 seconds) on a predefined radio frequency. The beacon is approximately 50 bytes (see SDR Adapter specification above) and contains the ship's node ID, sync state hash, and optional position.

When a ship receives a beacon from an unknown or previously out-of-range peer:
1. It adds the peer to its known-peers table
2. It compares the sync state hash to determine if sync is needed
3. If sync is needed, it initiates a sync session on a dedicated data frequency (separate from the beacon frequency to avoid contention)

When a ship stops hearing beacons from a peer (timeout: 5 minutes), it marks the peer as "out of range" but retains the sync state for future encounters.

---

## Gossip Protocol and Spray-and-Wait

### The Multi-Hop Problem

In a naval task group, not all ships can communicate directly with each other. Ship A may be within radio range of Ship B but not Ship C, while Ship B can reach both A and C. Messages from A to C must therefore transit through B.

Traditional flooding (broadcast every message to every neighbor) is wasteful of radio bandwidth. The system uses a gossip protocol inspired by the Spray-and-Wait algorithm (Spyropoulos, Psounis, and Raghavendra, 2005):

### Spray Phase

When a message is created, the originating node creates $L$ copies (default: $L = \lceil \log_2(N) \rceil$ where $N$ is the estimated fleet size). These copies are distributed to the first $L$ nodes encountered during sync sessions. Each copy includes a remaining-copies counter.

When a node with $k$ remaining copies meets a new peer, it gives $\lfloor k/2 \rfloor$ copies to the peer and retains $\lceil k/2 \rceil$ copies. This binary-split spray ensures rapid dissemination without flooding.

### Wait Phase

Once a node has only one copy remaining, it enters the "wait" phase: it holds the message until it directly encounters the destination node (or a node that has already delivered to the destination).

### Contact Probability Table

Each node maintains a contact probability table that records how frequently it encounters each other node:

$$P(contact\_with\_B) = \frac{\text{number of encounters with B in last } T \text{ hours}}{\text{total encounters in last } T \text{ hours}}$$

This probability is used to make intelligent relay decisions: if Ship B has a high contact probability with Ship C, Ship A should prefer routing messages for Ship C through Ship B.

### Delivery Receipts

When a message reaches its destination channel, a delivery receipt is generated and propagated back through the gossip network. Nodes that hold undelivered copies of the message can then delete them, freeing storage and reducing unnecessary retransmission.

### TTL-Based Cleanup

Messages that have not been delivered within a configurable Time-To-Live (TTL) are removed from the relay queue (but not from the originating node's database). Default TTLs:

| Priority | TTL |
|----------|-----|
| P0 FLASH | 24 hours |
| P1 IMMEDIATE | 48 hours |
| P2 PRIORITY | 7 days |
| P3-P6 | 30 days |

### Data Propagation Example

Consider a fleet with four nodes: Shore, Ship A, Ship B, Ship C.

```
Shore ---- SATCOM ---- Ship A ---- Radio ---- Ship B ---- Radio ---- Ship C
```

1. A message is created at Shore for a channel that includes personnel on Ship C
2. Shore syncs with Ship A via SATCOM (Ship A receives the message)
3. Ship A syncs with Ship B via radio (Ship B receives the message)
4. Ship B syncs with Ship C via radio (Ship C receives the message)
5. Delivery receipt propagates back: Ship C to Ship B to Ship A to Shore

Total propagation time depends on sync frequency and transport availability. In the best case (all links active), propagation takes seconds to minutes. In the worst case (ships only periodically in range), propagation takes hours to days. The system tolerates this delay gracefully because messages are persisted locally at every hop.

---

## Gateway Architecture

Gateways are not special servers. They are sync engine instances that happen to have multiple transport adapters attached.

### Shore Gateway

```
+------------------------------------+
|  SHORE GATEWAY                     |
|                                    |
|  +------------+  +--------------+  |
|  | Fiber      |  | SATCOM       |  |
|  | Adapter    |  | Adapter      |  |
|  +-----+------+  +------+-------+  |
|        |                |          |
|  +-----+----------------+-------+  |
|  |      SYNC ENGINE              |  |
|  |      (CRDT store, HLC,       |  |
|  |       delta sync, priority)   |  |
|  +-------------------------------+  |
+------------------------------------+
```

The shore gateway bridges the fiber LAN (connecting to headquarters, data centers, other shore facilities) and the SATCOM uplink (connecting to ships at sea). It runs the same sync engine as every other node; the only difference is that it has two transport adapters instead of one.

### Ship Gateway

```
+----------------------------------------------+
|  SHIP GATEWAY                                |
|                                              |
|  +----------+  +----------+  +------------+  |
|  | Ship LAN |  | SATCOM   |  | SDR Radio  |  |
|  | Adapter  |  | Adapter  |  | Adapter    |  |
|  +----+-----+  +----+-----+  +-----+------+  |
|       |             |              |          |
|  +----+-------------+--------------+-------+  |
|  |          SYNC ENGINE                     |  |
|  |          (CRDT store, HLC,               |  |
|  |           delta sync, priority)           |  |
|  +------------------------------------------+  |
+----------------------------------------------+
```

The ship gateway bridges three networks:
1. **Ship LAN**: connecting to client devices (desktops, tablets, phones via ship WiFi) within the ship
2. **SATCOM**: connecting to the shore gateway
3. **SDR Radio**: connecting to other ships in the task group

All three adapters feed into the same sync engine instance. A message created on a sailor's phone on Ship A flows: phone (WiFi) to Ship A gateway (LAN adapter) to sync engine to SATCOM adapter to shore gateway, and simultaneously to SDR radio adapter to Ship B gateway.

---

## Security Architecture

### End-to-End Encryption (E2EE)

All message content is encrypted end-to-end. The encryption key is known only to the members of the channel; no relay node, gateway, or server can read message content.

**Protocol options (implementation decision):**

| Option | Basis | Pros | Cons |
|--------|-------|------|------|
| Signal Protocol (libsignal) | Double Ratchet, X3DH key agreement | Industry standard; proven security; forward secrecy; post-compromise security | Complex key management; assumes online key exchange (X3DH) |
| Olm/Megolm (Matrix) | Double Ratchet variant for group messaging | Designed for group chats; used by Matrix/Element; scales to large groups | Less battle-tested than Signal; Megolm sacrifices some forward secrecy for group efficiency |
| NaCl/libsodium | Curve25519, XSalsa20, Poly1305 | Simple; well-audited; fast; deterministic | No built-in ratcheting; forward secrecy must be implemented separately |

The recommended approach is NaCl/libsodium for transport encryption (between sync engine instances) and a Double Ratchet variant (inspired by Signal/Olm) for message-level encryption (between users). This combines the simplicity and performance of NaCl for bulk data transfer with the forward secrecy properties of ratcheting for message confidentiality.

### Forward Secrecy

Each message exchange uses ephemeral keys that are deleted after use. If an adversary compromises a device, they can read messages stored on that device but cannot decrypt past messages that were encrypted with deleted ephemeral keys.

Formally: for each message $m_i$, a unique symmetric key $k_i$ is derived from the ratchet state. After encrypting $m_i$ with $k_i$, the key material used to derive $k_i$ is deleted. An adversary who obtains the device state at time $t$ can compute $k_j$ for $j \geq t$ but not for $j < t$.

### Hash Chains

Each message includes the SHA-256 hash of the previous message in its channel:

$$prev\_hash_i = SHA256(message_{i-1})$$

This creates a tamper-evident chain (analogous to a git commit history). If any message in the chain is modified, all subsequent hashes become invalid, and the tampering is immediately detectable by any node that holds the original chain.

This provides:
- **Tamper detection**: modifications to historical messages are detectable
- **Ordering proof**: the hash chain proves the sequence of messages
- **Consistency verification**: two nodes can verify they have the same message history by comparing hash chains

### Digital Signatures

Every message is signed with the author's Ed25519 private key. The signature covers all message fields (id, channel, author, HLC, classification, priority, content, prev_hash).

This provides:
- **Non-repudiation**: the author cannot deny having sent the message (the signature is verifiable with their public key, which is distributed during key exchange)
- **Integrity**: any modification to any field invalidates the signature
- **Authentication**: only the holder of the private key could have produced the signature

Ed25519 was chosen for its compact signature size (64 bytes), fast verification, and resistance to timing attacks.

### Zero-Knowledge Relay

Relay nodes (ships that forward messages between other ships in a mesh network) hold only encrypted blobs. They cannot:
- Read message content (encrypted with channel keys they do not possess)
- Forge messages (they do not possess the author's signing key)
- Modify messages (modification invalidates the Ed25519 signature)
- Determine message content from metadata (metadata is minimized to: destination channel ID, priority level, and size)

### Device Attestation

Before a device is admitted to the network, it must pass attestation checks:
- Verified boot chain (no rooted or jailbroken devices)
- Approved hardware model (military-issued or approved BYOD)
- Current software version (no outdated, vulnerable builds)
- Valid device certificate (issued by the fleet's certificate authority)

Attestation is re-verified periodically (default: every 24 hours) and on every sync session initiation.

### Remote Wipe

If a device is compromised, lost, or captured:
1. A wipe command is issued from the administration console
2. The command propagates through the sync network like any other P0 message
3. When the target device receives the command (or when it next connects), it:
   - Deletes all encryption keys
   - Overwrites the local database with random data
   - Resets to factory state

Even before the wipe command reaches the device, the compromised device's keys can be revoked at all other nodes, preventing it from syncing further.

### Classification Enforcement

The system supports multiple classification levels (UNCLASS, CONFIDENTIAL, SECRET). Enforcement is cryptographic:
- Each classification level has separate key material
- Users are provisioned with keys corresponding to their clearance level
- A user with CONFIDENTIAL clearance possesses UNCLASS and CONFIDENTIAL keys but not SECRET keys
- A user without the appropriate key literally cannot decrypt higher-classification messages; enforcement does not depend on application-layer access control that could be bypassed

### Audit Logging

All sync events, authentication events, and administrative actions are logged. The audit log records:
- Who sent a message (user ID, device ID)
- When (HLC timestamp)
- To which channel
- Message priority and classification
- Sync events (which nodes synced, when, how many messages exchanged)

The audit log does not record message content (which is E2EE and unreadable by the system). This enables accountability and forensic analysis without compromising message confidentiality.

---

## Why Not Blockchain

A natural question arises: if the system uses hash chains and digital signatures, why not use a blockchain? The answer is that blockchain solves a different problem.

### The Consensus Problem

Blockchains solve the problem of achieving consensus among untrusted parties:

$$\text{Blockchain solves: } P(\text{strangers agree on state} \mid \text{no trust})$$

This system solves a different problem:

$$\text{This system solves: } P(\text{trusted peers sync} \mid \text{disconnected})$$

A navy is not a trustless environment. The chain of command establishes trust. Nodes are authenticated with device certificates issued by a military certificate authority. There is no need for proof-of-work, proof-of-stake, or Byzantine Fault Tolerance because the threat model does not include malicious insider nodes attempting to subvert consensus. (Malicious insiders are handled by E2EE, signatures, and audit logging, not by consensus.)

### Why Consensus Fails at Sea

Consensus protocols (PBFT, Raft, Paxos, Nakamoto consensus) require a majority (or supermajority) of nodes to be reachable simultaneously. In a naval fleet:
- Ships are frequently disconnected from each other and from shore
- Submarines are unreachable for days or weeks
- Radio links are intermittent and low-bandwidth

A consensus protocol would either:
- Block (wait for enough nodes to respond), making the system unusable when disconnected
- Partition (fork into separate chains), requiring complex reconciliation when nodes reconnect

CRDTs avoid both problems: every node operates independently, and convergence is guaranteed by the mathematical properties of the data structure, not by a consensus protocol.

### Bandwidth Overhead

Consensus protocols generate significant overhead:
- Proof-of-work: computationally expensive and energy-intensive (irrelevant for military use)
- PBFT: requires $O(n^2)$ message exchanges per transaction (prohibitive on radio links)
- Raft: requires heartbeats and log replication to a majority (impossible when disconnected)

The system's approach (CRDTs with hash chains and signatures) requires zero overhead beyond the messages themselves. No consensus rounds, no leader election, no heartbeats.

### What This System Borrows from Blockchain

The system does borrow two valuable properties from blockchain technology:
1. **Hash chains**: tamper-evident message history (like blockchain's chain of blocks)
2. **Digital signatures**: non-repudiation and integrity (like blockchain's transaction signatures)

These properties are achieved without the consensus overhead. The result is a system with the auditability and tamper-resistance of a blockchain but the availability and partition-tolerance of a CRDT.

---

## Bandwidth Calculations

### Text Messages

A typical text message:
- Raw message content: ~200 characters = 200 bytes
- Message metadata (id, channel, author, HLC, classification, priority): ~150 bytes
- Ed25519 signature: 64 bytes
- Encryption overhead (nonce + MAC): ~40 bytes
- Total raw message: ~454 bytes
- After zstd compression (60-70% reduction on metadata/text): ~200 bytes

Conservative estimate: **200 bytes per compressed, encrypted, signed message**.

### SATCOM Throughput

At 256 kbps shared satellite link:

$$\text{Raw capacity} = \frac{256{,}000 \text{ bits/sec}}{8 \text{ bits/byte}} = 32{,}000 \text{ bytes/second}$$

With 25% of link allocated to messaging:

$$\text{Messaging capacity} = 32{,}000 \times 0.25 = 8{,}000 \text{ bytes/second}$$

$$\text{Messages per second} = \frac{8{,}000}{200} = 40 \text{ messages/second}$$

FLASH message delivery time (single message, P0 priority, immediate transmission):

$$\text{Delivery time} = \frac{200 \text{ bytes}}{32{,}000 \text{ bytes/sec}} + 0.6 \text{ sec (latency)} \approx 0.6 \text{ seconds}$$

Accounting for protocol overhead and scheduling: < 2 seconds for FLASH delivery on SATCOM.

### VLF Throughput

At 400 bps:

$$\text{Capacity} = \frac{400}{8} = 50 \text{ bytes/second}$$

A 140-character FLASH message (minimal metadata, no signature due to bandwidth constraint):

$$\text{Delivery time} = \frac{140}{50} \approx 2.8 \text{ seconds}$$

### Image Transfer

A compressed thumbnail image (50 KB) on SATCOM:

$$\text{Transfer time} = \frac{50{,}000 \text{ bytes}}{8{,}000 \text{ bytes/sec}} \approx 6.25 \text{ seconds}$$

A full-resolution image (500 KB) on SATCOM:

$$\text{Transfer time} = \frac{500{,}000}{8{,}000} \approx 62.5 \text{ seconds} \approx 1 \text{ minute}$$

### Daily Sync Budget

For a ship with 500 crew members, estimated daily message volume: 2,000 to 5,000 messages.

$$\text{Daily sync payload} = 5{,}000 \text{ messages} \times 200 \text{ bytes} = 1{,}000{,}000 \text{ bytes} = 1 \text{ MB}$$

$$\text{Sync time at 8 KB/s} = \frac{1{,}000{,}000}{8{,}000} = 125 \text{ seconds} \approx 2 \text{ minutes}$$

This is remarkably efficient. A full day's worth of text messages for a 500-person ship can be synced in approximately 2 minutes of SATCOM time. Even at reduced bandwidth (128 kbps with 25% allocation = 4 KB/s), full daily sync takes approximately 4 minutes.

### Radio (SDR) Throughput

At 9.6 kbps (conservative HF estimate):

$$\text{Capacity} = \frac{9{,}600}{8} = 1{,}200 \text{ bytes/second}$$

With Reed-Solomon FEC overhead (~20%):

$$\text{Effective capacity} = 1{,}200 \times 0.8 = 960 \text{ bytes/second}$$

$$\text{Messages per second} = \frac{960}{200} \approx 4.8 \text{ messages/second}$$

A sync session exchanging 100 messages (delta sync, not full database):

$$\text{Sync time} = \frac{100 \times 200}{960} \approx 21 \text{ seconds}$$

---

## Deployment Model

### Server Binary

The server is compiled as a single static Rust binary, approximately 50 MB. It has zero external dependencies: no JVM, no Python runtime, no Node.js, no Docker (though Docker deployment is supported for convenience). The binary includes:
- The sync engine
- All transport adapters
- The SQLite database engine (statically linked)
- A built-in web server for serving the client application
- An administration API

### Database

SQLite is used for all data persistence. SQLite was chosen because:
- Zero configuration (no database server to install, configure, or maintain)
- Single-file database (easy to backup, copy, and inspect)
- Proven reliability (billions of deployments; used in aircraft flight software, Android, iOS)
- FTS5 extension for full-text search of messages
- WAL mode for concurrent read/write access
- sqlite-vss extension for optional vector search (semantic search over messages)

Database size estimate for a ship with 500 crew over a 6-month deployment:

$$\text{Messages: } 5{,}000/\text{day} \times 180 \text{ days} = 900{,}000 \text{ messages}$$
$$\text{Size: } 900{,}000 \times 200 \text{ bytes} \approx 180 \text{ MB of message data}$$
$$\text{With indexes, FTS, metadata: } \approx 500 \text{ MB total}$$

This is trivially small for any modern storage device.

### Hardware Requirements

| Deployment | Hardware | CPU | RAM | Storage |
|------------|----------|-----|-----|---------|
| Ship server | Mini-PC or ruggedized server | Any modern x86_64 or ARM64 | 4 GB minimum, 8 GB recommended | 128 GB SSD |
| Shore server | Standard rack server or VM | 4+ cores | 16 GB | 1 TB SSD |
| Client device | Laptop, desktop, tablet, or phone | Any | 2 GB | 1 GB free |

The system is designed to run on hardware that ships already have or can easily procure. It does not require specialized military hardware, cloud infrastructure, or internet connectivity.

### Client Application

Two client options:

**Tauri Desktop Application**: a native desktop application built with Tauri (Rust backend, web frontend). Tauri produces small, fast native applications (typically 5-10 MB) that look and feel like native apps on Windows, macOS, and Linux. The Rust backend handles encryption, local caching, and sync with the local server.

**Progressive Web Application (PWA)**: a web application served from the ship's local server, accessible via any modern browser. The PWA is installed on the device and works offline, syncing with the local server when connected to the ship's WiFi. This option requires no software installation and works on any device with a browser (including personal phones with appropriate security controls).

Both clients connect to the local ship server (or shore server) over the ship's LAN or WiFi. They never connect directly to the internet.

---

## Tech Stack

| Component | Technology | Rationale |
|-----------|------------|-----------|
| Server language | Rust | Memory safety without garbage collection; single binary compilation; excellent async performance via tokio; no runtime dependencies |
| Database | SQLite + FTS5 + sqlite-vss | Zero-config; single file; proven reliability; full-text search built in; optional vector search for semantic queries |
| Sync protocol | Custom over QUIC or TCP | QUIC provides multiplexed streams, built-in encryption (TLS 1.3), and connection migration; TCP fallback for constrained environments |
| Encryption | libsodium (NaCl) for transport; Double Ratchet for messages | libsodium: audited, fast, simple API; Double Ratchet: forward secrecy, post-compromise security |
| Compression | zstd | High ratio, fast decompression, dictionary support for domain-specific content |
| Desktop client | Tauri (Rust + web frontend) | Small binary size, native performance, cross-platform, no Electron overhead |
| Web client | React (or SolidJS) served from localhost | Modern UI framework; served from local server, no internet dependency |
| Async runtime | tokio | Industry-standard Rust async runtime; handles concurrent transport adapters, sync sessions, and client connections |
| Serialization | MessagePack or CBOR | Compact binary serialization (smaller than JSON, faster to parse); schema-evolution support |
| Build and deployment | Single static binary via `cargo build --release --target x86_64-unknown-linux-musl` | Produces a fully static binary with no dynamic library dependencies; copy to target and run |

### Why Rust

Rust is chosen as the primary language for several reasons specific to this use case:

1. **Memory safety**: buffer overflows, use-after-free, and data races are compile-time errors in Rust. For a security-critical military system, this eliminates entire classes of vulnerabilities.
2. **Single binary**: Rust compiles to a single static binary with no runtime dependencies. This simplifies deployment enormously (no "install Java 17, then install PostgreSQL 15, then configure...").
3. **Performance**: Rust performs comparably to C/C++ without the safety risks. This matters for the sync engine, which must handle concurrent transport adapters, compression, encryption, and database operations.
4. **async/await**: Rust's async model (via tokio) is ideal for managing multiple concurrent transport connections without the overhead of OS threads.
5. **Cross-compilation**: Rust can cross-compile to ARM (for deployment on ARM-based ruggedized hardware) from an x86 development machine.

---

## Cross-References

- [[why-general-comms-matter]]: The argument for why this system is needed
- [[security-breaches]]: Security incidents that this architecture is designed to prevent
- [[comparative-analysis]]: How this architecture compares to existing systems in five countries
- [[india-military-comms]]: India-specific infrastructure that this system would integrate with
- [[us-military-comms]]: US systems (CANES, FLANK SPEED) that demonstrate the problem this architecture solves
- [[france-military-comms]]: France's Matrix/Tchap approach, the closest existing analog to this architecture
- [[russia-military-comms]]: Russian failures that validate the offline-first, resilient design
- [[israel-military-comms]]: Israeli pragmatic approach and its limitations
