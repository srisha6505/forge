# India: Military Communication Systems Analysis

## Overview

India's armed forces operate across three services (Army, Navy, Air Force) with distinct, often siloed communication infrastructures. Despite significant modernization investments over the past two decades, the Indian military lacks a unified, indigenous secure messaging and collaboration platform. The Indian Navy, in particular, has no purpose-built mobile-first communication tool, relying instead on a patchwork of legacy systems, email, and (unofficially) consumer applications.

---

## 1. Core Network Infrastructure

### 1.1 Navy Unified Domain (NUD)

The NUD is the Indian Navy's primary enterprise network, connecting shore establishments, dockyards, and naval headquarters. It provides:

- Intranet services across major naval bases (Mumbai, Visakhapatnam, Kochi, Karwar, Port Blair)
- Email, file sharing, and limited web-based applications
- Connectivity via a mix of leased lines, VSAT terminals, and dedicated fiber

**Limitations:**
- Primarily shore-based; shipboard connectivity is intermittent and bandwidth-constrained
- No native mobile access layer
- Limited to unclassified and restricted-level traffic in most configurations
- Ships at sea rely on satellite links (INMARSAT, GSAT-7) with severe bandwidth throttling

### 1.2 Naval Enterprise Wide Area Network (NEWN)

NEWN is the Navy's backbone wide-area network linking all major commands and establishments.

- Fiber-optic and satellite hybrid topology
- Supports voice, data, and video conferencing between shore nodes
- Managed by the Directorate of Naval Communication and Information Technology (DNCIT)
- Integration with the broader Defence Communication Network (DCN) is ongoing but incomplete

### 1.3 Air Force Network (AFNET)

AFNET is the Indian Air Force's dedicated network, operationalized around 2010.

- Fiber-optic backbone connecting all IAF bases
- Supports secure voice, data, and video
- Uses indigenous encryption (developed with DRDO/CAIR assistance)
- Considered the most advanced of the three service networks
- The Navy has no direct equivalent of AFNET's integrated architecture

### 1.4 Army Static Switched Communication Network (ASCON)

ASCON is the Indian Army's primary communication backbone.

| Phase | Technology | Status | Coverage |
|-------|-----------|--------|----------|
| Phase I | Analog microwave | Operational (legacy) | Northern and Western commands |
| Phase II | Digital microwave | Operational | Extended coverage |
| Phase III | IP-based, OFC backbone | Operational | Nationwide |
| Phase IV | Software-defined, high-bandwidth | Under deployment | All commands, forward areas |

ASCON Phase IV (contracted to Tata Advanced Systems and L&T) is a significant upgrade, introducing IP-based switching, software-defined networking, and higher bandwidth to forward areas. However, ASCON remains Army-specific; the Navy does not benefit from this investment.

### 1.5 Defence Communication Network (DCN)

The DCN is the tri-service strategic communication backbone managed by the Defence Communication & Information Technology Agency (DCITA), under the Integrated Defence Staff (IDS).

- National Fibre Optic Network (NFON) backbone
- Connects the three service headquarters, joint commands, and the Chief of Defence Staff (CDS) secretariat
- Supports secure video conferencing, data exchange, and strategic messaging
- Intended to enable joint operations and interoperability

**Limitations:**
- Rollout has been slower than planned
- End-user experience is dominated by legacy terminals and thick-client applications
- No mobile or field-deployable component for naval personnel at sea
- Interoperability between service-specific networks (NUD, AFNET, ASCON) remains limited

---

## 2. Email and Messaging Infrastructure

### 2.1 NIC Email System

The National Informatics Centre (NIC) provides the default email service for all Indian government organizations, including the Ministry of Defence (MoD) and the armed forces.

| Parameter | Detail |
|-----------|--------|
| Domains | `@nic.in`, `@gov.in`, `@navy.gov.in`, `@mod.gov.in` |
| Platform | NIC Mail (custom webmail, based on open-source components) |
| Authentication | Username/password; limited two-factor adoption |
| Hosting | NIC data centers (Delhi, Hyderabad, Pune) |
| Encryption | TLS in transit; no end-to-end encryption |
| Storage | Limited per-user quotas (typically 1-2 GB) |
| Mobile access | Basic webmail; no dedicated mobile app with push notifications |
| Classification | Unclassified to Restricted only |

**Limitations:**
- User interface is dated and lacks modern collaboration features (threaded conversations, shared channels, integrated file editing)
- No offline capability
- Attachment size limits are restrictive (typically 10-25 MB)
- No integration with document management, task tracking, or workflow tools
- Personnel routinely maintain personal email accounts on commercial services for convenience, creating shadow IT risks
- No message recall, expiry, or remote wipe capability

### 2.2 SAI (Secure Application for the Internet)

SAI is a secure messaging application developed by the Indian Army's Corps of Signals / Military Intelligence.

- Intended as a WhatsApp replacement for military personnel
- End-to-end encryption (reportedly using AES-256)
- Available on Android; iOS support limited or absent
- Hosted on NIC infrastructure

**Status and limitations:**
- Adoption has been low; most estimates suggest fewer than 50,000 active users across the Army
- User experience is significantly inferior to commercial alternatives
- No voice or video calling in early versions
- No desktop or web client
- Limited group management features
- No cross-service adoption (Navy and Air Force do not use SAI)

### 2.3 ASIGMA (Army Software for Instant Messaging Application)

ASIGMA is another Army-developed secure instant messaging platform.

- Developed by the Army Cyber Group
- Operates on the Army's internal network (ASCON)
- Text messaging with basic file sharing
- Classified-network capable

**Status and limitations:**
- Restricted to Army users on ASCON
- No interoperability with Navy or Air Force systems
- Limited feature set compared to commercial messaging platforms
- No mobile-first design; primarily desktop-based

### 2.4 SAMBHAV (Secure Army Mobile Bharat Version)

SAMBHAV is a more recent secure communication platform developed for the Indian Army.

- Smartphone-based secure communication
- Supports voice, video, and messaging
- Designed for use on both military and commercial networks with appropriate security layers
- Developed with assistance from DRDO and industry partners

**Status and limitations:**
- Pilot deployment phase as of 2025; full-scale rollout timeline unclear
- Army-specific; no Navy or Air Force adoption planned
- Interoperability with existing systems (SAI, ASIGMA, eOffice) is not confirmed
- Performance on low-bandwidth networks (typical at sea) is untested

### 2.5 M-Sigma

M-Sigma is a messaging application reportedly developed for the Indian Navy by a domestic vendor.

- Limited public information available
- Intended for internal naval communication
- Status of deployment is unclear; no large-scale adoption reported
- Not integrated with NUD or DCN in a meaningful way

### 2.6 Summary: Messaging Platform Fragmentation

| Platform | Service | Network | Mobile | Voice/Video | E2EE | Status |
|----------|---------|---------|--------|-------------|------|--------|
| SAI | Army | Internet/NIC | Android only | Limited | Yes (claimed) | Low adoption |
| ASIGMA | Army | ASCON (internal) | No | No | N/A (internal net) | Operational, limited |
| SAMBHAV | Army | Hybrid | Yes | Yes | Yes (claimed) | Pilot phase |
| M-Sigma | Navy | Unknown | Unknown | Unknown | Unknown | Unclear |
| NIC Email | All services | Internet | Webmail only | No | No | Operational |

The critical observation is that none of these platforms have achieved the adoption, feature parity, or cross-service interoperability necessary to displace consumer alternatives like WhatsApp.

---

## 3. eOffice Adoption

eOffice is the Government of India's digital workplace solution, developed by NIC, for paperless office operations.

### Modules
- File Management System (eFile)
- Knowledge Management System (KMS)
- Collaboration and Messaging System (CAMS)
- Personnel Information Management System (PIMS)
- Tour Management System
- Leave Management System

### Defence adoption
- MoD headquarters and several defence establishments have adopted eOffice for file movement and approvals
- Army, Navy, and Air Force headquarters use eFile for noting and correspondence
- Adoption is uneven; many units still operate parallel paper-based processes
- eOffice is a file/workflow management tool, not a communication or collaboration platform
- No real-time messaging, no presence indicators, no mobile-optimized interface
- Does not address the need for operational communication (orders, situational awareness, coordination)

---

## 4. Unofficial Communication: The WhatsApp Problem

Despite repeated orders from the MoD, Army HQ, and Naval HQ banning the use of WhatsApp and other commercial messaging applications for official communication, their use remains pervasive.

### Why personnel use WhatsApp
- Ubiquity: every smartphone has it pre-installed or easily available
- Feature richness: voice/video calls, group chats, file sharing, location sharing
- Reliability: works on low-bandwidth connections, supports offline queuing
- Zero training required
- Cross-platform (Android, iOS, web)

### Known risks
- Metadata harvested by Meta (WhatsApp parent company); stored on servers outside India
- No organizational control over data retention, forwarding, or screenshots
- Personnel share operational details, movement schedules, deployment orders via WhatsApp groups
- No message recall or remote wipe by the organization
- Susceptible to social engineering, SIM-swapping, and state-sponsored interception
- Multiple documented incidents of sensitive information leakage (see [[security-breaches]])

### Policy vs. reality
The gap between policy (ban commercial messaging) and reality (everyone uses WhatsApp) is the single most important market signal for a secure naval communication product. Personnel will not adopt a secure alternative unless it matches or exceeds the user experience of WhatsApp while operating within the security constraints of military networks.

---

## 5. Major Infrastructure Programs

### 5.1 Project Varun (STL / Sterlite Technologies)

Project Varun is a submarine optical fiber cable (OFC) network project to connect the Indian mainland with the Andaman and Nicobar Islands and Lakshadweep.

- Contracted to STL (Sterlite Technologies Limited)
- Provides high-bandwidth, low-latency connectivity to the tri-service command at Port Blair (HQANC)
- Strategic significance: the Andaman and Nicobar Command is India's only operational tri-service command and a critical node for Indian Ocean surveillance
- Complements satellite links; provides resilient terrestrial-grade connectivity to island territories

### 5.2 ASCON Phase IV

As noted above, ASCON Phase IV modernizes the Army's communication backbone with IP-based, software-defined infrastructure. While Army-specific, it sets a benchmark that the Navy's communication infrastructure has not yet matched.

### 5.3 Network for Spectrum (NFS)

NFS is a joint project between the Indian Armed Forces and telecom operators (primarily BSNL, with participation from other operators).

- The armed forces vacated spectrum in the 1800 MHz and 2100 MHz bands for commercial 4G/5G use
- In exchange, telecom operators build and maintain a nationwide OFC network for defence use
- Provides high-bandwidth connectivity to cantonments, air bases, and naval establishments
- Approximately 60,000 km of OFC planned
- Status: significant portions operational; full completion timelines have slipped repeatedly

### 5.4 GSAT-7R Satellite

GSAT-7R is the Indian Navy's dedicated military communication satellite, replacing the aging GSAT-7 (Rukmini).

| Parameter | Specification |
|-----------|--------------|
| Launch mass | 4,410 kg |
| Launch date | November 2025 |
| Launch vehicle | GSLV Mk III (LVM3) |
| Orbit | Geostationary (GEO) |
| Design life | 15 years |
| Coverage | Indian Ocean Region (IOR) |
| Bands | UHF, S-band, C-band, Ku-band |
| Capacity | Significant upgrade over GSAT-7; exact throughput classified |
| Primary users | Indian Navy (ships, submarines, aircraft, shore stations) |
| Secondary users | Indian Coast Guard, limited tri-service use |

**Significance:**
- GSAT-7R provides the satellite backbone for naval communication across the Indian Ocean
- Enables beyond-line-of-sight (BLOS) communication for ships and submarines
- Supports data, voice, and video; bandwidth remains constrained compared to terrestrial networks
- A secure messaging platform must be designed to function within GSAT-7R's bandwidth envelope, optimizing for low-bandwidth, high-latency links

### 5.5 BEL SDR-Tac Radios

Bharat Electronics Limited (BEL) has developed Software Defined Radio (SDR) variants for tactical military communication.

- SDR-Tac (Tactical): man-portable and vehicle-mounted variants
- SDR-NC (Naval Communication): ship-mounted variant for the Navy
- Supports multiple waveforms (frequency hopping, broadband, narrowband)
- Indigenous development with DRDO/DEAL (Defence Electronics Applications Laboratory) collaboration
- Intended to replace legacy Harris, Rohde & Schwarz, and Tadiran radios in Indian service

**Relevance to secure messaging:**
- SDR radios provide the physical layer; they do not include an application-layer messaging or collaboration capability
- A secure messaging platform can potentially integrate with SDR data channels for last-mile connectivity in tactical scenarios

---

## 6. Enterprise IT Systems (Siloed)

### 6.1 SAP FIS (Financial Information System)

- SAP-based ERP deployed across defence establishments for financial management
- Manages budgeting, accounting, procurement, and payments
- Does not include any communication or collaboration features
- Operated as a standalone system with no integration to operational communication networks

### 6.2 ILMS (Integrated Logistics Management System)

- Manages inventory, spare parts, maintenance scheduling, and logistics across naval platforms
- Multiple variants in use across services
- No messaging or collaboration layer

### 6.3 SPARSH (System for Pension Administration, Rajya Sainik Boards, and Hospitality)

- Defence pension disbursement and management system
- Web-based portal for pensioners
- No relevance to operational communication

### 6.4 The Silo Problem

Each of these systems operates independently with:
- Separate authentication credentials
- No single sign-on (SSO)
- No unified notification system
- No shared communication bus
- No mobile-first interfaces

Personnel must log into multiple systems to perform routine tasks, with no way to receive alerts, approvals, or status updates through a common platform.

---

## 7. The Critical Gap: Why the Navy Needs an Indigenous Communication Platform

### 7.1 No indigenous messaging platform

The Indian Navy has no operational, Navy-wide, purpose-built secure messaging application. M-Sigma's status is unclear, and Army platforms (SAI, ASIGMA, SAMBHAV) are neither designed for nor adopted by the Navy.

### 7.2 No collaboration suite

There is no equivalent of Microsoft Teams, Slack, or Mattermost deployed across the Navy. Personnel cannot:
- Create persistent channels for projects, units, or operations
- Share and collaboratively edit documents in real time
- Conduct threaded discussions with searchable history
- Integrate bots, workflows, or automated alerts

### 7.3 No mobile-first tool

Naval personnel, especially officers ashore and those in transit, have no sanctioned mobile application for official communication. The result is predictable: they use WhatsApp, Signal, or Telegram, exposing operational information to foreign-owned platforms and metadata harvesting.

### 7.4 No offline-capable communication

Ships at sea experience intermittent connectivity. No existing Indian military communication tool supports:
- Offline message composition and queuing
- Store-and-forward synchronization when satellite links become available
- Bandwidth-optimized message encoding for constrained SATCOM links

### 7.5 No integration layer

Existing systems (eOffice, SAP FIS, ILMS, NIC Email) cannot share notifications or workflow events through a common communication platform. Each operates as an island, requiring manual context switching.

### 7.6 Summary of the opportunity

The Indian Navy needs a platform that combines:

1. Secure, end-to-end encrypted messaging (text, voice, video)
2. Persistent channels and collaboration spaces
3. Mobile-first design with offline capability
4. Bandwidth optimization for satellite links (GSAT-7R)
5. Integration with existing Navy IT systems (NUD, eOffice, ILMS)
6. Indigenous development and sovereign data hosting
7. Compliance with Indian defence security standards
8. User experience competitive with WhatsApp to drive voluntary adoption

For documented security incidents that reinforce this urgency, see [[security-breaches]].

---

## References

1. Ministry of Defence Annual Reports (2020-2025)
2. Parliamentary Standing Committee on Defence Reports
3. DRDO Technology Focus publications
4. Indian Navy official website (indiannavy.nic.in)
5. Defence ProAc Business News
6. ISRO GSAT-7R mission documentation
7. BEL Annual Reports and product catalogs
8. NIC official documentation
9. Comptroller and Auditor General (CAG) Reports on Defence Communication
10. Open-source defence journalism (The Print, India Today, Economic Times Defence)
