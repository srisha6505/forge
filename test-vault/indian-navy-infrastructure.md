# Indian Navy Communication Infrastructure

This document provides a detailed mapping of the Indian Navy's existing communication infrastructure, covering shore networks, satellite systems, shipboard communications, submarine communications, naval aviation links, fleet composition, base locations, and administrative systems. Understanding this infrastructure is essential for designing a secure communication product that integrates with (rather than replaces) the Navy's existing systems.

Cross-references: [[procurement-paths]], [[matrix-protocol-analysis]], [[technical-architecture]]

---

## Table of Contents

1. [Shore Network: NEWN](#shore-network-newn)
2. [Shore Network: NFS](#shore-network-nfs)
3. [Tri-Service: DCN](#tri-service-dcn)
4. [Satellite: GSAT-7 Rukmini](#satellite-gsat-7-rukmini)
5. [Satellite: GSAT-7R / CMS-03](#satellite-gsat-7r--cms-03)
6. [Ship Communications](#ship-communications)
7. [Submarine Communications](#submarine-communications)
8. [Naval Air Communications](#naval-air-communications)
9. [Fleet Composition](#fleet-composition)
10. [Major Naval Bases](#major-naval-bases)
11. [Locations Without Fiber Connectivity](#locations-without-fiber-connectivity)
12. [ERP and Administrative Systems](#erp-and-administrative-systems)
13. [Network Topology Diagram](#network-topology-diagram)

---

## Shore Network: NEWN (Navy Enterprise Wide Network)

### Overview

The Navy Enterprise Wide Network (NEWN) is the Indian Navy's dedicated private network backbone connecting all major shore establishments. It is the primary data transport layer for shore-to-shore communication and serves as the termination point for ship-to-shore satellite links.

### Technical Specifications

| Parameter | Detail |
|---|---|
| Naval bases connected | 30+ |
| Network sites | 1,500 |
| Locations | 44 |
| Secure data centres | 40 |
| Endpoints | 30,000+ |
| Network type | 100% private (no shared/public infrastructure) |
| Backbone technology | Optical fiber |
| Key technologies | Wide-area optical fiber sensing, software-defined applications |

### Project Varun: NEWN Overhaul

The NEWN underwent a major overhaul under "Project Varun," executed by STL (Sterlite Technologies Limited). Key facts:

- **Completion timeline**: 12 months (an aggressive timeline for a network of this scale)
- **Scope**: Complete refresh of the optical fiber backbone, network equipment, and management systems across all 44 locations
- **Technologies deployed**:
  - Wide-area optical fiber sensing: fiber itself acts as a distributed sensor, detecting physical intrusions (tapping, bending, cutting) along the cable route. This provides a layer of physical security unique to the NEWN.
  - Software-defined networking (SDN): enables dynamic bandwidth allocation, traffic prioritization, and centralized network management
  - Software-defined WAN (SD-WAN): optimizes traffic routing across the wide-area network
- **Security features**:
  - Physical intrusion detection via fiber sensing
  - Encrypted transport (likely AES-256 at the optical transport layer)
  - No connection to public internet (complete air-gap from civilian networks)
  - 40 secure data centres with redundancy and disaster recovery

### Implications for Our Product

- **Bandwidth**: The NEWN provides high-bandwidth (multi-gigabit) connectivity between shore establishments. Our product can assume ample bandwidth for shore-to-shore communication.
- **Latency**: Optical fiber latency across India is in the range of 10 to 30 milliseconds between any two bases. Real-time messaging and voice/video are fully supported.
- **Security**: The NEWN is air-gapped. Our product must operate in a fully offline (no internet) environment when deployed on shore establishments.
- **Deployment**: Our product's shore-side servers would be deployed in the NEWN's 40 secure data centres. Integration with NEWN's SDN infrastructure may enable traffic prioritization for our product's data flows.
- **Endpoints**: 30,000+ endpoints represent the potential user base at shore establishments. Each endpoint is a workstation connected to the NEWN.

---

## Shore Network: NFS (Network for Spectrum)

### Overview

The Network for Spectrum (NFS) is a pan-India optical fiber network built exclusively for the Indian Armed Forces. Unlike the NEWN (which is Navy-specific), NFS is a tri-service network used by the Army, Navy, and Air Force, as well as other security agencies.

### Technical Specifications

| Parameter | Detail |
|---|---|
| Total fiber optic cable | 58,000 km |
| Construction authority | BSNL (Bharat Sanchar Nigam Limited) |
| Total project cost | >Rs 13,000 Crore (~$1.56 Billion) |
| Cable technology | Intrusion-proof (tamper-evident, armored) |
| Suppliers | HFCL (supplied 11,000 km, deployed 3,500 km), other vendors |

### Design and Architecture

NFS was conceived as a "barter" arrangement: the Ministry of Defence released spectrum in the 1800 MHz and 2100 MHz bands to the Department of Telecommunications (DoT) for commercial 4G/5G deployment. In exchange, BSNL (the government-owned telecom operator) was tasked with building a 58,000 km fiber optic network exclusively for defence use.

Key architectural features:

- **Separate fiber**: NFS uses dedicated fiber strands; it is not a virtual overlay on BSNL's commercial network
- **Intrusion-proof cable**: The cable uses tamper-evident sheathing and armored construction. Any physical attempt to access the fiber (tapping, splicing) causes detectable signal degradation
- **Redundant routing**: Critical routes have diverse path options to ensure connectivity even if one cable segment is cut
- **HFCL's role**: HFCL (Himachal Futuristic Communications Limited) was a major supplier, providing 11,000 km of cable and deploying 3,500 km. HFCL also supplied network equipment for several NFS nodes

### Implications for Our Product

- **Tri-service reach**: NFS connects Army, Navy, and Air Force establishments. If our product is deployed on NFS, it could potentially serve as a tri-service secure messaging platform (expanding the addressable market significantly).
- **BSNL as gatekeeper**: BSNL manages the NFS infrastructure. Any deployment on NFS requires coordination with BSNL's defence division.
- **Bandwidth**: NFS provides lower bandwidth than NEWN for naval-specific traffic, as bandwidth is shared across services. However, it provides backup connectivity if NEWN experiences outages.

---

## Tri-Service: DCN (Defence Communication Network)

### Overview

The Defence Communication Network (DCN) is a tri-service communication network that connects the three service headquarters, the Ministry of Defence, and key operational commands across India.

### Technical Specifications

| Parameter | Detail |
|---|---|
| Launch date | July 2016 |
| Built by | HCL Infosystems |
| Contract value | ~Rs 600 Crore (~$72 Million) |
| Entities connected | 111 |
| Geographic coverage | Pan-India, including Ladakh, Northeast India, Andaman & Nicobar Islands |
| Services supported | Voice (encrypted), video conferencing (encrypted), data transfer |

### Architecture

DCN is built as a secure overlay network that uses a combination of:

- Dedicated fiber optic links (where available, riding NFS or NEWN infrastructure)
- Dedicated satellite links (for remote locations like Ladakh, Northeast, island territories)
- Encrypted tunnels over all links

DCN provides:

- **Secure voice**: Encrypted VoIP between connected entities
- **Secure video conferencing**: Multi-point video conferencing for operational briefings
- **Secure data transfer**: File transfer and email between connected entities
- **Network management**: Centralized monitoring and management from the DCN Network Operations Centre (NOC)

### Connected Entities

The 111 entities include:

- Ministry of Defence (South Block, New Delhi)
- Integrated Defence Staff HQ (New Delhi)
- Army HQ, Naval HQ, Air Force HQ (all in New Delhi)
- All operational commands (Army: Northern, Western, Central, Eastern, Southern, South Western, Training; Navy: Western, Eastern, Southern, Andaman & Nicobar; Air Force: Western, Eastern, Central, Southern, South Western, Training, Maintenance)
- Key formations, bases, and installations across India

### Implications for Our Product

- **Existing infrastructure**: DCN already provides basic secure communication (voice, video, data) across 111 entities. Our product must offer capabilities beyond what DCN provides: modern messaging UX, group collaboration, file sharing with previews, threaded conversations, offline operation, mobile access.
- **Integration**: Our product could integrate with DCN for transport, using DCN's encrypted links for inter-service communication while using NEWN for Navy-internal communication.
- **HCL relationship**: HCL built DCN and likely maintains it. Understanding HCL's role and any potential for partnership is important.

---

## Satellite: GSAT-7 "Rukmini" (2013)

### Overview

GSAT-7, named "Rukmini," is India's first dedicated military communication satellite, built by ISRO and operated by the Indian Navy. It provides communication coverage across the Indian Ocean Region (IOR), enabling shore-to-ship, ship-to-ship, and ship-to-air communication.

### Technical Specifications

| Parameter | Detail |
|---|---|
| Launch date | September 2013 |
| Launch vehicle | Ariane 5 ECA (Arianespace, Kourou) |
| Satellite bus | ISRO I-2K (Indian 2,000 kg class) |
| Launch mass | 2,650 kg |
| Frequency bands | UHF, S-band, C-band, Ku-band |
| Transponders | 11 |
| Orbital position | Geostationary, 74 degrees East longitude |
| Footprint | Approximately 2,000 nautical miles from the Indian coastline |
| Coverage area | Indian Ocean Region, from the coast of East Africa to the Strait of Malacca |
| Operational test | TROPEX 2014: simultaneously connected 60 ships and 75 aircraft |
| Cost (satellite) | ~Rs 185 Crore (~$22 Million) |
| Cost (launch) | ~Rs 480 Crore (~$58 Million) |
| Total cost | ~Rs 665 Crore (~$80 Million) |
| Design life | 9 years (extended beyond original life as of 2026) |

### Frequency Bands and Usage

| Band | Frequency Range | Typical Use |
|---|---|---|
| UHF | 300 MHz to 3 GHz | Ship-to-ship tactical communication, submarine communication (limited) |
| S-band | 2 to 4 GHz | Medium data rate ship-to-shore communication |
| C-band | 4 to 8 GHz | Primary data backbone for ship-to-shore communication |
| Ku-band | 12 to 18 GHz | High data rate communication, video conferencing, large file transfer |

### Capacity and Bandwidth

GSAT-7's 11 transponders provide a total capacity estimated at:

- UHF: limited to narrowband voice and low-rate data (a few kbps per channel)
- S-band: approximately 2 to 5 Mbps aggregate
- C-band: approximately 10 to 30 Mbps aggregate
- Ku-band: approximately 20 to 50 Mbps aggregate

These are shared across all connected ships and aircraft. During TROPEX 2014, 60 ships and 75 aircraft shared this capacity simultaneously, implying per-platform bandwidth of approximately 100 to 500 kbps for typical operations, with burst capability for priority traffic.

### Implications for Our Product

- **Bandwidth constraint**: 100 to 500 kbps per ship is the primary design constraint for our product's ship-to-shore synchronization. Every byte transmitted must be justified. See [[matrix-protocol-analysis]] for why Matrix's HTTP-based protocol is too chatty for this link.
- **Latency**: Geostationary satellite latency is approximately 600 ms round-trip (two hops: ship to satellite to shore). Our product must be tolerant of high latency.
- **Coverage**: GSAT-7 covers the Indian Ocean Region. Ships operating beyond this footprint (e.g., deployments to the Mediterranean, Pacific) would need alternative satellite links.
- **Aging asset**: GSAT-7 is beyond its 9-year design life (launched 2013). GSAT-7R is the replacement. Our product should be designed for GSAT-7R's capabilities.

---

## Satellite: GSAT-7R / CMS-03 (November 2025)

### Overview

GSAT-7R (also designated CMS-03, Communication Satellite 03) is the replacement and significant upgrade to GSAT-7. It was launched in November 2025 and is now operational, providing enhanced communication capabilities for the Indian Navy.

### Technical Specifications

| Parameter | Detail |
|---|---|
| Launch date | November 2025 |
| Launch vehicle | LVM3-M5 (formerly GSLV Mk III), from Sriharikota (SDSC SHAR) |
| Mass | 4,410 kg (heaviest Indian military communication satellite launched from Indian soil) |
| Frequency bands | UHF, S-band, C-band, Extended C-band, Ku-band |
| Mission life | 15 years (through approximately 2040) |
| Notable features | Collapsible antenna systems, indigenous 1,200-litre propulsion tank |
| Orbital position | Geostationary (exact longitude classified, likely near 74 degrees East) |

### Improvements Over GSAT-7

| Feature | GSAT-7 (2013) | GSAT-7R (2025) |
|---|---|---|
| Mass | 2,650 kg | 4,410 kg (+66%) |
| Bands | UHF, S, C, Ku | UHF, S, C, Extended C, Ku |
| Design life | 9 years | 15 years |
| Launch origin | Arianespace (French Guiana) | ISRO LVM3 (India) |
| Antenna systems | Fixed | Collapsible (larger aperture, more gain) |
| Propulsion | Standard | Indigenous 1,200-litre tank |
| Throughput (estimated) | ~60 to 80 Mbps total | ~150 to 300 Mbps total (estimated) |

### Extended C-band

The addition of Extended C-band (5.85 to 6.425 GHz uplink, 3.625 to 4.2 GHz downlink) is significant. Extended C-band:

- Provides additional capacity beyond standard C-band
- Is less susceptible to rain fade than Ku-band (important for monsoon operations in the Indian Ocean)
- Can use smaller shipboard antennas than standard C-band for equivalent performance (due to higher gain from GSAT-7R's collapsible antennas)

### Implications for Our Product

- **More bandwidth**: GSAT-7R's estimated 150 to 300 Mbps total capacity (shared across the fleet) translates to approximately 500 kbps to 2 Mbps per ship during normal operations, with burst capability up to 5 to 10 Mbps for priority traffic. This is a 3x to 5x improvement over GSAT-7.
- **15-year horizon**: GSAT-7R will be operational through approximately 2040. Our product's satellite communication design should be optimized for GSAT-7R's capabilities as the baseline.
- **All-weather**: Extended C-band improves monsoon-season reliability, reducing communication blackout periods that affected GSAT-7's Ku-band links during heavy rain.

---

## Ship Communications

### SATCOM via GSAT-7R / ICNS (BEL)

**ICNS (Integrated Communication and Navigation System)** is BEL's comprehensive shipboard communication suite. ICNS integrates:

- SATCOM terminals (C-band, Ku-band, and now Extended C-band via GSAT-7R)
- HF radio (1.6 to 30 MHz)
- VHF radio (30 to 300 MHz)
- UHF radio (300 MHz to 3 GHz)
- Navigation systems (GPS, GLONASS, NavIC/IRNSS)
- Internal communication (intercom, 1MC, sound-powered phones)

**SATCOM Topologies Supported by ICNS**:

| Topology | Description | Use Case |
|---|---|---|
| Star | All ships communicate through a shore hub (earth station) | Normal operations, shore-controlled traffic |
| Mesh | Ships communicate directly with each other via satellite | Task force operations, reduced latency between ships |
| Hybrid | Combination of star and mesh | Fleet operations with mixed requirements |

### BEL SDR-Tac (Software Defined Radio, Tactical)

SDR-Tac is BEL's flagship tactical radio system for the Indian Navy.

**Technical Specifications**:

| Parameter | Detail |
|---|---|
| Channels | Four simultaneous channels |
| Modes | Multi-mode (voice, data, mixed) |
| Bands | Multi-band (HF, VHF, UHF) |
| Deployment | Ship-borne, aircraft-borne (MH-60R Seahawk) |
| Communication types | Ship-to-ship, ship-to-shore, ship-to-air |
| Relay capability | Each ship acts as a relay node, extending network range |
| Procurement value | Rs 490 Crore (~$59 Million) |
| Units procured | 260+ |
| Platforms | Major warships, MH-60R Seahawk helicopters |

**Network Architecture**: SDR-Tac creates a mobile ad-hoc network (MANET) across a task force. Each ship equipped with SDR-Tac acts as a relay node, meaning that a message from Ship A can reach Ship C by hopping through Ship B if Ship C is out of direct radio range. This relay capability is particularly valuable for task force operations where ships are spread across a wide area.

**Data Rates**: SDR-Tac supports data rates from a few kbps (HF) to several Mbps (UHF line-of-sight). For our product, SDR-Tac's data channel could serve as an alternative transport when SATCOM is unavailable or congested.

### Sandesh System

The Sandesh system provides VLF (Very Low Frequency) and HF (High Frequency) broadcast reception capability on all ships and submarines.

- **VLF reception**: Receives one-way broadcasts from shore VLF transmitters. VLF signals can penetrate seawater to a shallow depth, enabling reception by submarines at periscope depth or shallow immersion.
- **HF reception**: Receives HF broadcasts from shore HF transmitters.
- **One-way**: Sandesh is a receive-only system. Ships and submarines receive broadcasts; they do not transmit via Sandesh.
- **Data rate**: VLF is extremely low bandwidth, approximately 300 to 400 bps. HF broadcast can achieve 2.4 to 9.6 kbps.
- **Use case**: Operational orders, situation updates, weather broadcasts, and emergency communications from shore to fleet.

### Saral System

The Saral system handles cipher encoding and decoding for naval communications.

- **Function**: Encrypts outgoing messages and decrypts incoming messages
- **Deployment**: Installed on all warships and submarines
- **Key management**: Uses a hierarchical key management system with key distribution from shore crypto centres
- **Integration**: Works in conjunction with the SAMSS system for message routing

### SAMSS (Shore-to-Afloat Message Switching System)

SAMSS handles the switching and routing of formatted messages between the War Room (Naval Operations Centre) and communication centres aboard ships.

- **Function**: Routes messages based on priority, classification, and addressee
- **Message format**: Uses naval message format (likely derived from the ACP-127 standard for military message handling)
- **Priority levels**: Flash, Immediate, Priority, Routine, Deferred
- **Routing**: From the War Room to the appropriate communication centre (shore or shipboard), which then transmits via the appropriate link (SATCOM, HF, VLF)

### HF-Interface Unit

The HF-Interface Unit enables secure voice communication over HF radio links.

- **Function**: Digitizes voice, encrypts it, and transmits it over HF radio. On the receiving end, it decrypts and reconstructs the voice signal.
- **Voice quality**: Vocoder-based compression results in intelligible but low-quality voice (similar to a satellite phone)
- **Range**: HF radio can achieve ranges of several thousand kilometers, especially using skywave propagation (signal bouncing off the ionosphere)
- **Use case**: Secure voice between ships and shore when SATCOM is unavailable or reserved for data traffic

### Hughes JUPITER VSAT

The Indian Navy uses Hughes JUPITER VSAT terminals for supplementary satellite communication.

| Parameter | Detail |
|---|---|
| Bands | C-band, Ku-band |
| Protocol | IP-MPLS hybrid |
| Function | Provides additional bandwidth for internet, email, and welfare communication |
| Deployment | Major warships, some shore establishments |

**IP-MPLS Hybrid**: The Hughes JUPITER system uses a hybrid IP and MPLS (Multi-Protocol Label Switching) architecture. MPLS provides quality-of-service guarantees for priority traffic (operational messages), while IP handles best-effort traffic (welfare communication, general internet).

### Implications for Our Product

Our product must integrate with this communication ecosystem, not replace it:

1. **Transport agnostic**: The product must be able to use any available transport: SATCOM (via ICNS), HF data (via SDR-Tac or HF-Interface), NEWN fiber (at shore), and potentially VLF for critical one-way alerts.
2. **Priority-aware**: Must map message priorities to the existing naval priority system (Flash, Immediate, Priority, Routine, Deferred) and work with SAMSS routing logic.
3. **Crypto integration**: Must either integrate with Saral (the existing crypto system) or provide its own encryption that is approved by SAG/DRDO. A dual-layer approach (application-layer encryption from our product + transport-layer encryption from Saral) provides defense in depth.
4. **Bandwidth adaptation**: Must dynamically adapt to available bandwidth, from 400 bps (VLF, receive-only) through 9.6 kbps (HF) to 2 Mbps (SATCOM/GSAT-7R).

---

## Submarine Communications

### The Submarine Communication Challenge

Submarine communication is fundamentally constrained by physics. Seawater is highly conductive and absorbs electromagnetic radiation. The only radio frequencies that can penetrate seawater to any useful depth are Extremely Low Frequency (ELF, 3 to 30 Hz) and Very Low Frequency (VLF, 3 to 30 kHz). Even VLF penetrates only to a depth of approximately 10 to 20 meters.

### Current Submarine Communication Capabilities

| Mode | Direction | Bandwidth | Depth Requirement | Platform |
|---|---|---|---|---|
| VLF reception | Shore to submarine | ~300 to 400 bps | Periscope depth or shallow immersion (~10 to 20m) | All submarines |
| HF radio | Bidirectional | 2.4 to 9.6 kbps | Surfaced or mast-exposed | All submarines |
| SATCOM | Bidirectional | 64 kbps to 2 Mbps | Surfaced or mast-exposed | Scorpene-class, Arihant-class |
| UHF | Short-range bidirectional | Up to several Mbps | Surfaced | All submarines |

### VLF Communication

- **Existing VLF Station**: INS Kattabomman, Tirunelveli, Tamil Nadu. One of the few VLF transmitter stations in the world. Transmits at approximately 18 to 20 kHz with very high power (hundreds of kilowatts).
- **New VLF Station**: Under construction at Vikarabad, Telangana. Expected to be operational by approximately 2027. This station will provide redundancy and extended coverage.
- **One-way only**: VLF is used exclusively for shore-to-submarine broadcasts. Submarines receive VLF signals using a trailing wire antenna (a long wire antenna trailed behind the submarine while submerged). Transmitting at VLF frequencies requires antenna structures hundreds of meters long and power in the hundreds of kilowatts range, making it impractical from a submarine.
- **Data rate**: Approximately 300 to 400 bps. At this rate, a typical 500-character message takes approximately 10 to 15 seconds to transmit. Only short, pre-formatted messages (operational orders, position reports, alerts) are practical.

### Submarine Types in the Indian Navy

| Class | Type | Number | Origin | Communication Suite |
|---|---|---|---|---|
| Sindhughosh (Kilo-class) | Diesel-electric | ~7 (some being decommissioned) | Russia/Soviet Union | VLF, HF, UHF, limited SATCOM |
| Kalvari (Scorpene-class) | Diesel-electric | 6 | France (Naval Group) / MDL Mumbai | VLF, HF, SATCOM (Thales), UHF |
| Arihant-class | Nuclear SSBN | 2 to 3 (classified) | Indigenous | VLF, HF, SATCOM, UHF (details classified) |
| S5-class (under construction) | Nuclear SSN | Under construction | Indigenous | Details classified |

### Implications for Our Product

Submarine communication presents the most extreme design challenge:

1. **Receive-only while submerged**: At depth, a submarine can only receive VLF broadcasts (300 to 400 bps). Our product must support a "receive-only" mode where the submarine receives priority messages via VLF without any ability to send.
2. **Burst synchronization**: When a submarine surfaces or comes to periscope depth for SATCOM/HF communication, it has a limited window (minutes) before it must submerge again to avoid detection. Our product must perform a highly efficient "burst sync" during this window: sending queued outgoing messages and receiving queued incoming messages in the minimum possible time.
3. **Store-and-forward**: Messages from shore to submarine may be queued for hours or days until the submarine's next communication window. Our product must support asynchronous store-and-forward with guaranteed delivery.
4. **Security**: Submarine communication is among the most sensitive in any navy. The encryption and key management for submarine communication must meet the highest standards. Post-quantum cryptography consideration is relevant here given the 15 to 20 year operational life of submarines.

---

## Naval Air Communications

### Aircraft Types and Communication Systems

| Aircraft | Base(s) | Role | Communication Suite |
|---|---|---|---|
| MiG-29K/KUB | INS Hansa, Goa | Carrier-borne fighter | HF, UHF, tactical datalink |
| P-8I Poseidon | INS Rajali, Arakkonam (Tamil Nadu) | Maritime patrol, ASW | HF, UHF, SATCOM, tactical datalink, Link-16 compatible |
| MH-60R Seahawk | Multiple bases | Multi-role helicopter (ASW, anti-surface, SAR) | HF, UHF, SATCOM, BEL SDR-Tac |
| Dornier 228 | Multiple coastal bases | Maritime surveillance | HF, VHF, UHF |
| Ka-31 | Carrier-based | AEW (Airborne Early Warning) | UHF datalink, tactical communication |
| Sea King | Multiple bases (being retired) | ASW, utility | HF, UHF |
| ALH Dhruv (Naval variant) | Multiple bases | Utility, SAR | VHF, UHF |

### BEL SDR-Tac on MH-60R Seahawk

The MH-60R Seahawk is the Indian Navy's newest helicopter acquisition (24 units from Lockheed Martin/Sikorsky, deliveries ongoing). BEL's SDR-Tac has been installed on the MH-60R, providing:

- Secure voice and data communication with ships and shore
- Multi-band operation (HF, VHF, UHF) in a single unit
- Relay capability: the MH-60R can act as a relay node between ships or between a ship and shore
- Integration with the Seahawk's onboard mission systems

### P-8I Poseidon Communication

The P-8I is the Indian Navy's most capable communication platform in the air:

- SATCOM: Direct satellite communication via Ku-band terminal, providing broadband connectivity during long-range maritime patrol missions
- HF: Long-range communication for operations beyond SATCOM footprint
- UHF: Short-range tactical communication with ships and other aircraft
- Link-16 compatible: NATO-standard tactical datalink (limited use in Indian Navy context, but the capability exists)

### Implications for Our Product

- **Air-to-ground/ship messaging**: Pilots and mission crews on P-8I and MH-60R could use our product for secure messaging with shore operations centres and ship command teams.
- **Real-time position/status updates**: Aircraft conducting maritime patrol could send and receive status updates, intelligence reports, and tasking orders through our product.
- **Bandwidth**: Airborne platforms have better SATCOM connectivity than ships (no horizon limitations, larger Ku-band antennas on P-8I). Our product can assume higher bandwidth (1 to 10 Mbps) for airborne platforms.
- **Integration**: Airborne deployment would require a lightweight client (possibly tablet-based) that integrates with the aircraft's existing communication suite.

---

## Fleet Composition

### Aircraft Carriers

| Ship | Class | Displacement | Crew | Status |
|---|---|---|---|---|
| INS Vikramaditya | Modified Kiev-class | 45,000 tons | ~1,500 | Active (Western Fleet) |
| INS Vikrant | Indigenous Aircraft Carrier 1 (IAC-1) | 43,000 tons | ~1,600 | Active (commissioned August 2022) |

### Destroyers

| Class | Number | Displacement | Crew | Notable |
|---|---|---|---|---|
| Kolkata-class (Project 15A) | 3 | 7,400 tons | ~300 | BEL CMS, ICNS |
| Visakhapatnam-class (Project 15B) | 4 | 7,400 tons | ~300 | Enhanced BEL CMS, ICNS |
| Delhi-class (Project 15) | 3 | 6,200 tons | ~350 | Older CMS, being upgraded |
| Rajput-class (Project 61ME) | 1 (remaining) | 4,900 tons | ~300 | Soviet-era, nearing decommission |

### Frigates

| Class | Number | Displacement | Crew | Notable |
|---|---|---|---|---|
| Shivalik-class (Project 17) | 3 | 6,200 tons | ~250 | Stealth features, BEL systems |
| Talwar-class | 6 | 4,000 tons | ~180 | Russian-origin, upgraded with Indian systems |
| Nilgiri-class (Project 17A) | 7 (planned, 4 launched) | 6,670 tons | ~250 | Advanced stealth, BEL CMS and ICNS |

### Corvettes

| Class | Number | Displacement | Crew | Notable |
|---|---|---|---|---|
| Kamorta-class (Project 28) | 4 | 3,300 tons | ~150 | ASW corvette, BEL systems |
| Kora-class (Project 25A) | 4 | 1,400 tons | ~120 | Missile corvette |
| Khukri-class (Project 25) | 4 | 1,350 tons | ~100 | Missile corvette |
| Abhay-class | 2 | 485 tons | ~40 | ASW corvette, smaller |

### Patrol Vessels, Amphibious Ships, and Others

| Type | Approximate Number | Notes |
|---|---|---|
| Offshore Patrol Vessels (OPVs) | 10+ | Saryu-class, other classes |
| Fast Attack Craft | 10+ | Missile boats, torpedo boats |
| Landing Ships | 5+ | INS Jalashwa (LPD), Shardul-class (LST) |
| Mine Countermeasure Vessels | 6+ | Pondicherry-class, Karwar-class |
| Survey and Research Vessels | 8+ | Sandhayak-class, Makar-class |
| Tankers and Support Ships | 5+ | Deepak-class fleet tankers |

### Submarines

| Class | Type | Number | Notes |
|---|---|---|---|
| Sindhughosh (Kilo-class) | SSK (diesel-electric) | ~7 | Russian-origin, some being decommissioned |
| Kalvari (Scorpene-class) | SSK (diesel-electric) | 6 | French design, MDL-built, newest conventional subs |
| Arihant-class | SSBN (nuclear ballistic missile) | 2 to 3 | Indigenous, classified |
| S5-class | SSN (nuclear attack) | Under construction | Indigenous, classified |

### Total Fleet Summary

| Category | Approximate Number |
|---|---|
| Aircraft carriers | 2 |
| Destroyers | 11 |
| Frigates | 16 |
| Corvettes | 14 |
| Patrol vessels and fast attack craft | 20+ |
| Amphibious ships | 8+ |
| Mine warfare vessels | 6+ |
| Survey/research vessels | 8+ |
| Tankers/support ships | 5+ |
| Submarines | ~16 |
| **Total commissioned warships** | **~130+** |
| Naval aircraft | ~200+ |

---

## Major Naval Bases (Fiber-Connected via NEWN)

### Delhi: Naval Headquarters / IHQ MoD (Navy)

- **Location**: South Block and adjacent buildings, New Delhi
- **Role**: Headquarters of the Indian Navy, office of the Chief of Naval Staff
- **Communication**: Connected to NEWN, NFS, and DCN. Primary nexus for all naval communication policy and operations.
- **Key directorates**: DNC (Directorate of Naval Communications), ACNS (CSNO), NIIO, Naval Intelligence

### Mumbai: Western Naval Command HQ

- **Location**: Mumbai (various installations)
- **Installations**: Naval Dockyard (Mumbai), INS Angre (shore base), WESEE
- **Role**: Headquarters of Western Naval Command, responsible for Arabian Sea operations
- **Fleet**: Western Fleet (aircraft carrier INS Vikramaditya, destroyers, frigates, submarines)
- **Communication**: Connected to NEWN backbone. Major SATCOM earth station for GSAT-7/7R. WESEE provides technical evaluation capability.

### Visakhapatnam: Eastern Naval Command HQ

- **Location**: Visakhapatnam, Andhra Pradesh
- **Installations**: INS Circars (shore base), Submarine Base (INS Virbahu), Naval Dockyard (Visakhapatnam)
- **Role**: Headquarters of Eastern Naval Command, responsible for Bay of Bengal and eastern Indian Ocean operations
- **Fleet**: Eastern Fleet (aircraft carrier INS Vikrant, destroyers, frigates, submarines)
- **Communication**: Connected to NEWN backbone. Major SATCOM earth station.

### Kochi: Southern Naval Command HQ

- **Location**: Kochi, Kerala
- **Installations**: INS Venduruthy (shore base), Cochin Shipyard Limited (CSL), Naval Ship Repair Yard
- **Role**: Headquarters of Southern Naval Command, responsible for training
- **Notable**: ILMS (Integrated Logistics Management System) was built in-house by a team in Kochi
- **Communication**: Connected to NEWN backbone.

### Karwar / INS Kadamba: Largest Naval Base (Project Seabird)

- **Location**: Karwar, Karnataka
- **Project Seabird**: A massive naval base expansion project (one of the largest naval infrastructure projects in Asia)
  - Phase I: Completed (can berth up to 11 warships)
  - Phase IIA: Underway (expanding to berth 32 warships, 23 submarines)
  - Final capacity: Will be India's largest naval base, potentially the largest in Asia
- **Strategic importance**: Western seaboard, deep-water harbor, away from Pakistan border (unlike Mumbai)
- **Communication**: Connected to NEWN backbone. Major communication hub for Western Fleet operations.

### Goa / INS Hansa: Naval Aviation

- **Location**: Dabolim, Goa
- **Role**: Primary naval air station, home base for MiG-29K carrier fighters
- **Communication**: Connected to NEWN backbone. Air-ground communication systems for naval aviation.

### Port Blair: Andaman and Nicobar Command

- **Location**: Port Blair, Andaman Islands
- **Role**: Headquarters of the Andaman and Nicobar Command (tri-service operational command)
- **Strategic importance**: Controls the approaches to the Strait of Malacca, India's most strategically important maritime chokepoint monitoring position
- **Communication**: Connected to NEWN via undersea fiber optic cable (Chennai to Port Blair) and SATCOM backup. Critical for monitoring Chinese naval movements in the eastern Indian Ocean.

### Ezhimala: Indian Naval Academy

- **Location**: Ezhimala, Kerala
- **Role**: Indian Naval Academy (INA), training establishment for officer cadets
- **Communication**: Connected to NEWN backbone.

---

## Locations Without Fiber Connectivity

### Ships at Sea

- **Connectivity**: SATCOM only (via GSAT-7R through ICNS)
- **Bandwidth**: 100 kbps to 2 Mbps per ship (typical), up to 5 to 10 Mbps burst
- **Latency**: ~600 ms round-trip (geostationary satellite)
- **Availability**: Near-continuous within GSAT-7R footprint (Indian Ocean Region)
- **Backup**: HF radio (2.4 to 9.6 kbps), SDR-Tac relay

### Submarines

- **Submerged**: VLF reception only (~300 to 400 bps, one-way shore to sub)
- **Periscope depth**: VLF reception + limited HF transmission
- **Surfaced**: HF bidirectional (2.4 to 9.6 kbps) + SATCOM (64 kbps to 2 Mbps)
- **Constraint**: Surfacing or coming to periscope depth exposes the submarine to detection; communication windows are minimized

### Aircraft in Flight

- **Fighters (MiG-29K)**: UHF/HF radio only, no SATCOM, limited to voice and low-rate data
- **Patrol aircraft (P-8I)**: HF, UHF, and Ku-band SATCOM (broadband, 1 to 10 Mbps)
- **Helicopters (MH-60R)**: HF, UHF, SDR-Tac (multi-band, relay capable)
- **Other aircraft (Dornier 228, ALH Dhruv)**: VHF/UHF radio only

### Lakshadweep (INS Dweeprakshak)

- **Location**: Kavaratti, Lakshadweep Islands
- **Connectivity**: SATCOM only (no undersea or overland fiber)
- **Role**: Naval detachment responsible for Lakshadweep island security
- **Bandwidth**: Limited SATCOM (VSAT terminal, estimated 512 kbps to 2 Mbps)

### Smaller Andaman and Nicobar Island Outposts

- **Locations**: Car Nicobar (INS Baaz, the southernmost air station), Campbell Bay, and other small outposts
- **Connectivity**: SATCOM or microwave links to Port Blair
- **Bandwidth**: Limited (256 kbps to 1 Mbps)

### Forward / Temporary Deployments

- **Deployments**: Ships and personnel deployed for exercises (RIMPAC, Malabar, MILAN), humanitarian assistance/disaster relief (HADR), or anti-piracy operations (Gulf of Aden)
- **Connectivity**: Portable VSAT terminals (Hughes JUPITER or similar)
- **Bandwidth**: 512 kbps to 2 Mbps (depending on terminal and satellite availability)

---

## ERP and Administrative Systems

### SAP FIS (Financial Information System)

| Parameter | Detail |
|---|---|
| Launched | August 2012 |
| Implementation partner | Wipro |
| System | SAP ERP (Financial Accounting module) |
| Organizational units covered | 600+ |
| Scope | Budgeting, expenditure tracking, financial reporting across the Navy |
| Network | Runs on NUD (Naval Unified Domain) intranet, accessed via NEWN-connected workstations |

### ILMS (Integrated Logistics Management System)

| Parameter | Detail |
|---|---|
| Developed by | In-house by a team of 10 developers at Southern Naval Command, Kochi |
| Platforms covered | 250 ships and shore establishments |
| Functions | Inventory management, procurement, supply chain, maintenance scheduling |
| Notable | Built entirely in-house, saving significant cost compared to external development |
| Network | Runs on NUD intranet |

### ILMS Air Version 2.0

| Parameter | Detail |
|---|---|
| Launched | October 2017 |
| Scope | Logistics management for naval aviation (aircraft, spare parts, maintenance) |
| Cost saving | Rs 30 Crore saved versus external development |
| Network | Runs on NUD intranet |

### SPARSH (System for Pension Administration, Raksha)

| Parameter | Detail |
|---|---|
| Scope | Tri-service pension, leave management, payroll |
| Type | Centralized web-based system |
| Operator | CGDA (Controller General of Defence Accounts) |
| Network | Accessible via NUD and DCN |

### e-CRs and e-SDs

- **e-CRs (Electronic Confidential Reports)**: Digital system for officer performance evaluation reports, replacing paper-based CRs
- **e-SDs (Electronic Service Documents)**: Digital sailor service records, including postings, qualifications, medical records, and disciplinary records

### NUD (Naval Unified Domain)

NUD is the Indian Navy's intranet that connects all the above systems. It runs on the NEWN backbone and provides:

- Web portal for accessing ILMS, SAP FIS, e-CRs, e-SDs, and other applications
- Email (likely a military email system)
- Document management
- Collaboration tools (basic; this is where our product would add significant value)

### Implications for Our Product

- **Integration opportunity**: Our product could integrate with NUD to provide modern messaging and collaboration capabilities that complement the existing ERP and administrative systems. For example, a logistics officer could receive an ILMS alert in our messaging product, discuss it with colleagues, and take action, all within a unified interface.
- **Authentication**: Our product should support integration with the Navy's existing identity management system (likely Active Directory or a similar directory service running on NUD).
- **Data classification**: Messages in our product that reference ERP data must carry appropriate classification markings consistent with the data's sensitivity level.

---

## Network Topology Diagram

The following ASCII diagram illustrates how the Indian Navy's communication systems interconnect:

```
                            SPACE SEGMENT
    ========================================================================

                         GSAT-7R (CMS-03)
                    GEO @ ~74 deg East, 2025
                    UHF/S/C/ExtC/Ku-band
                         /    |    \
                        /     |     \
                       /      |      \
              C/Ku-band   C/Ku-band   UHF/S-band
              uplink/     uplink/      uplink/
              downlink    downlink     downlink
                     /       |        \
    ================|========|=========|====================================
                    |        |         |
                SHORE    SHIPS AT    SUBMARINES
                EARTH      SEA      (surfaced)
                STATION


                          SHORE SEGMENT
    ========================================================================

    [Naval HQ Delhi]
         |
         | NEWN Fiber (via Project Varun / STL)
         | NFS Fiber (58,000 km, via BSNL)
         | DCN (HCL, 111 entities)
         |
    +----|-----+-----------+-----------+-----------+----------+----------+
    |          |           |           |           |          |          |
    [Mumbai]  [Vizag]    [Kochi]   [Karwar]    [Goa]    [Port Blair] [Ezhimala]
    Western   Eastern    Southern   INS        INS       A&N          INA
    Naval Cmd Naval Cmd  Naval Cmd  Kadamba    Hansa     Command
    HQ        HQ         HQ        (Seabird)  (Aviation)
    WESEE     Sub Base                                    Undersea
    Dockyard  Dockyard                                    fiber from
                                                          Chennai
    |              |            |           |
    | SATCOM       | SATCOM     |           |
    | Earth Stn    | Earth Stn  |           |
    |              |            |           |
    +------+-------+            |           |
           |                    |           |
           | GSAT-7R            |           |
           | C/Ku/ExtC          |           |
           |                    |           |


                          AFLOAT SEGMENT
    ========================================================================

    SATCOM (GSAT-7R via ICNS/BEL)          HF/VHF/UHF (SDR-Tac/BEL)
         |                                       |
    +----+----+----+----+----+                   |
    |    |    |    |    |    |                   |
    CV  DDG  FFG  SSK  SSBN OPV         Ship-to-ship relay
    (2) (11) (16) (6)  (2+) (10+)       (each ship = MANET node)

    CV  = Aircraft Carrier (INS Vikramaditya, INS Vikrant)
    DDG = Destroyer (Kolkata, Visakhapatnam, Delhi class)
    FFG = Frigate (Shivalik, Talwar, Nilgiri class)
    SSK = Diesel Submarine (Scorpene/Kalvari class)
    SSBN= Nuclear Submarine (Arihant class)
    OPV = Offshore Patrol Vessel


                        SUBMARINE SEGMENT
    ========================================================================

    Shore VLF Transmitters                     Shore HF Transmitters
    (INS Kattabomman, Tirunelveli)             (Multiple locations)
    (New station: Vikarabad, ~2027)
         |                                          |
         | VLF (~18-20 kHz)                         | HF (1.6-30 MHz)
         | ~300-400 bps                              | 2.4-9.6 kbps
         | ONE-WAY (shore to sub)                    | Bidirectional
         | Penetrates to ~10-20m depth               | Requires surfacing
         |                                          |
    +----+----+                                +----+----+
    |         |                                |         |
    SSK      SSBN                             SSK      SSBN
    (at       (at                              (surfaced (surfaced
    periscope  periscope                       or mast   or mast
    depth)     depth)                          exposed)  exposed)


                        AVIATION SEGMENT
    ========================================================================

    Shore Air Stations                    Carrier Air Wing
    (INS Hansa/Goa,                       (INS Vikrant,
     INS Rajali/Arakkonam)                 INS Vikramaditya)
         |                                     |
         | UHF/VHF                              | UHF
         | (Ground-to-air)                      | (Ship-to-air)
         |                                     |
    +----+----+----+----+              +----+----+
    |    |    |    |    |              |         |
    P-8I MiG  MH-  Do-  ALH          MiG-29K   Ka-31
    Pos- 29K  60R  228  Dhruv                   AEW
    eidon      Sea-
    (SATCOM+   hawk
     HF/UHF)  (SDR-Tac)


                    INTERNAL SHIP NETWORK (per warship)
    ========================================================================

    +-------------------------------------------------------------------+
    |                         WARSHIP                                    |
    |                                                                    |
    |  [ICNS - BEL]                                                      |
    |  Integrated Communication and Navigation System                    |
    |       |            |            |           |                       |
    |    SATCOM        HF Radio    VHF/UHF     Navigation                |
    |    Terminal      Terminal    Terminal     (GPS/NavIC)               |
    |       |            |            |                                   |
    |  [SDR-Tac - BEL]                                                   |
    |  4-channel, multi-band tactical radio                              |
    |       |                                                            |
    |  [Saral] Cipher encoding/decoding                                  |
    |       |                                                            |
    |  [SAMSS] Message switching (War Room <> Comms Centre)              |
    |       |                                                            |
    |  [Sandesh] VLF/HF broadcast reception                              |
    |       |                                                            |
    |  [HF-Interface] Secure voice over HF                               |
    |       |                                                            |
    |  [CMS - BEL] Combat Management System                              |
    |  (integrates sensors, weapons, comms)                              |
    |       |                                                            |
    |  [Ship LAN] Ethernet network connecting all above                  |
    |       |                                                            |
    |  [Workstations] ~50-400 per ship (depending on class)              |
    |       |                                                            |
    |  [OUR PRODUCT: Secure Messaging Server]                            |
    |  Runs on ship LAN, syncs via SATCOM/HF when available              |
    |  Provides messaging, collaboration, file sharing                   |
    |  End-to-end encrypted (Olm/Megolm or SAG-approved)                 |
    |  Offline-first: full functionality without connectivity             |
    +-------------------------------------------------------------------+


                    ADMINISTRATIVE / ERP OVERLAY
    ========================================================================

    [NUD - Naval Unified Domain] (Intranet, runs on NEWN)
         |
    +----+----+----+----+----+----+
    |    |    |    |    |    |    |
    SAP  ILMS ILMS SPARSH e-CRs e-SDs
    FIS       Air         (officer (sailor
    (Wipro)   v2.0        reports) records)

    All accessible from NEWN-connected workstations at shore establishments.
    Ships access NUD applications when connected via SATCOM (limited bandwidth).


                    PRODUCT INTEGRATION POINTS
    ========================================================================

    [Our Product] integrates at the following points:

    1. SHORE: Deployed in NEWN data centres (40 secure DCs)
       - Server: Homeserver instance per base
       - Clients: Desktop app on 30,000+ NEWN workstations
       - Sync: Real-time via NEWN fiber backbone (low latency, high bandwidth)

    2. SHIP: Deployed on ship LAN
       - Server: Lightweight homeserver per ship (Rust-based, minimal footprint)
       - Clients: Desktop/tablet app on ship workstations
       - Sync to shore: Via SATCOM (GSAT-7R, 100kbps-2Mbps, ~600ms latency)
       - Sync to nearby ships: Via SDR-Tac data channel (ad-hoc)
       - Offline: Full local operation when disconnected

    3. SUBMARINE: Deployed on submarine internal network
       - Server: Lightweight homeserver (minimal footprint, hardened)
       - Receive: VLF broadcast reception for priority messages (~400bps)
       - Burst sync: SATCOM/HF when surfaced (minutes-long windows)
       - Offline: Primary operating mode

    4. AIRCRAFT: Lightweight client on mission tablets
       - Sync: Via aircraft SATCOM (P-8I) or SDR-Tac relay (MH-60R)
       - Offline: Pre-loaded mission data, post-mission sync
```

---

*Last updated: March 2026*

Cross-references: [[procurement-paths]], [[matrix-protocol-analysis]], [[technical-architecture]], [[comparative-analysis]]
