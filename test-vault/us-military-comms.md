# United States: Military Communication Systems Analysis

## Overview

The United States Department of Defense (DoD) operates the world's largest and most complex military communication infrastructure. Spanning three classification tiers, multiple cloud environments, and a global network of bases, ships, aircraft, and forward-deployed forces, the US system represents both the gold standard and a cautionary tale. Despite spending billions on enterprise IT, the DoD still struggles with shipboard connectivity, cross-classification data transfer, mobile device authentication, and the fundamental tension between security and usability.

---

## 1. Three-Tier Network Architecture

The DoD's network architecture is organized by classification level. Each tier is a physically or cryptographically separated network with its own infrastructure, authentication mechanisms, and access controls.

### 1.1 NIPRNET (Non-classified Internet Protocol Router Network)

| Parameter | Detail |
|-----------|--------|
| Classification | Unclassified / FOUO (For Official Use Only) / CUI (Controlled Unclassified Information) |
| Impact Level | IL2, IL4, IL5 |
| Users | ~3.5 million DoD personnel, contractors, and civilian employees |
| Access | CAC (Common Access Card) authentication; accessible from most DoD workstations |
| Services | Email, web browsing (filtered), collaboration tools (Teams), file sharing, enterprise applications |
| Connectivity | Global; connects all DoD installations via DISA-managed backbone |
| Internet access | Yes, through filtered gateways (JRSS / Thunderdome) |

NIPRNET is the workhorse network for day-to-day administrative, logistics, and unclassified operational work. Most DoD personnel interact with NIPRNET daily.

### 1.2 SIPRNET (Secret Internet Protocol Router Network)

| Parameter | Detail |
|-----------|--------|
| Classification | SECRET |
| Impact Level | IL6 |
| Users | ~500,000+ (estimated) |
| Access | CAC + SIPRNET token; dedicated terminals in SCIFs or approved spaces |
| Services | Classified email, operational planning tools, intelligence products, C2 systems |
| Connectivity | Global; physically separated from NIPRNET |
| Internet access | None; air-gapped from public internet |

SIPRNET carries the majority of classified operational traffic. Access requires being in a physically secured space with a dedicated terminal; there is no mobile SIPRNET access for general users.

### 1.3 JWICS (Joint Worldwide Intelligence Communications System)

| Parameter | Detail |
|-----------|--------|
| Classification | TOP SECRET / SCI |
| Impact Level | IL6+ |
| Users | ~100,000+ (estimated; primarily intelligence community and senior leadership) |
| Access | TS/SCI clearance, dedicated terminals in hardened SCIFs |
| Services | Intelligence dissemination, national-level C2, compartmented programs |
| Connectivity | Global; most restricted access |
| Internet access | None |

JWICS is the most restricted tier, serving the intelligence community and senior military leadership. Access terminals are scarce and located only in specially constructed facilities.

### 1.4 Cross-Domain Solutions (CDS)

Moving information between classification tiers requires a Cross-Domain Solution, which is a combination of hardware, software, and procedural controls.

- Solutions include the High Assurance Internet Protocol Encryptor (HAIPE) devices, Trusted Thin Client, and various NSA-approved CDS
- The process is slow, manual, and introduces significant latency into decision-making
- A classified report or intelligence product must be "downgraded" or redacted before transfer to a lower classification network
- This remains one of the most significant friction points in DoD communication

---

## 2. Email and Collaboration

### 2.1 Defense Enterprise Email (DEE)

Defense Enterprise Email consolidated the DoD's previously fragmented email systems (each service and agency ran its own) into a single Exchange-based platform.

| Parameter | Detail |
|-----------|--------|
| Platform | Microsoft Exchange / Outlook |
| Authentication | CAC (Common Access Card) with PKI certificates |
| Users | ~4 million (across all services and defence agencies) |
| Hosting | DISA-operated data centers; migrating to cloud |
| Classification | NIPRNET (unclassified/CUI) |
| Storage | Typically 4 GB per user (expanded in cloud migration) |
| Mobile access | Limited; CAC readers for smartphones exist but are cumbersome |

**Limitations:**
- CAC-based authentication on mobile devices requires a physical card reader or derived credentials, creating significant friction
- Email remains the dominant communication mode for official correspondence, resulting in massive inbox volumes
- No native real-time messaging integration in early DEE; Teams now supplements this
- Classified email on SIPRNET uses a separate Exchange infrastructure

### 2.2 DoD365 / Microsoft Teams

The DoD adopted Microsoft 365 (DoD365) as its enterprise collaboration platform, including Teams, SharePoint Online, OneDrive, and the full Office suite.

| Parameter | Detail |
|-----------|--------|
| Contract | Defense Enterprise Office Solution (DEOS) |
| Contract value | $4.4 billion (10-year ceiling) |
| Contractor | General Dynamics IT (GDIT), with Microsoft as technology provider |
| Users | ~4 million across DoD |
| Classification (initial) | NIPRNET (IL5) |
| Features | Chat, channels, video conferencing, file sharing, SharePoint integration, Power Automate |

### 2.3 FLANK SPEED (Navy M365)

FLANK SPEED is the US Navy's specific implementation of Microsoft 365.

| Parameter | Detail |
|-----------|--------|
| Users | ~900,000 (Navy and Marine Corps) |
| Rollout began | 2021 |
| Platform | Microsoft Teams, Outlook, SharePoint, OneDrive |
| Classification | NIPRNET (IL5) |
| Authentication | CAC-based; Derived Credentials for mobile |
| Managed by | Navy Program Executive Office for Digital and Enterprise Services (PEO Digital) |

FLANK SPEED replaced the Navy's legacy NMCI (Navy Marine Corps Intranet) email and collaboration tools. It represents a significant modernization, but its utility is sharply limited at sea.

### 2.4 Teams on SIPRNET (IL6)

Microsoft Teams deployment on SIPRNET has been in progress since 2022.

- Impact Level 6 (SECRET) certification achieved
- Rollout to SIPRNET users began in 2022 and continued through 2023
- Provides classified chat, channels, and video conferencing for the first time
- Limited to SIPRNET terminals in secured spaces; no mobile access
- Represents a major step forward for classified collaboration but does not solve the shipboard or tactical edge problem

---

## 3. Secure Messaging Initiatives

### 3.1 Wickr (AWS)

Wickr was an end-to-end encrypted messaging application acquired by Amazon Web Services (AWS) in 2021.

| Parameter | Detail |
|-----------|--------|
| Acquisition | AWS acquired Wickr, June 2021 |
| DoD use | Adopted by several DoD components for secure mobile messaging |
| Features | E2EE messaging, voice, video, file sharing, ephemeral messages |
| Classification | Unclassified; used as a supplement to official channels |
| FedRAMP | Wickr Gov achieved FedRAMP High authorization |
| Shutdown | AWS announced sunset of Wickr services, effective March 2025 |

**Significance:**
- Wickr was the closest the DoD came to a WhatsApp-equivalent secure messaging tool
- Its shutdown left a gap in mobile secure messaging capability
- No direct replacement has been designated; Teams on mobile with Derived Credentials is the de facto fallback, but adoption friction is high

### 3.2 Matrix / Element (Evaluation)

The Matrix protocol (with the Element client) has been evaluated by several DoD and intelligence community entities.

- Matrix is an open, decentralized communication protocol supporting E2EE
- Element (formerly Riot.im) is the primary Matrix client
- The protocol's federation model, open-source codebase, and E2EE capabilities attracted interest
- France's adoption of Matrix (via Tchap; see [[france-military-comms]]) provided a proof of concept for sovereign military messaging
- As of 2025, no large-scale DoD deployment of Matrix/Element has been confirmed
- Concerns cited include: maturity of the ecosystem, integration with CAC/PKI authentication, support and maintenance model, and the challenge of operating a federated system within DoD's centralized security architecture

### 3.3 CVR (Commercial Virtual Remote)

CVR was a COVID-19 pandemic stopgap collaboration tool.

- Deployed rapidly in 2020 to enable remote work for DoD personnel during lockdowns
- Provided video conferencing and basic collaboration for unclassified use
- Based on commercial technologies with expedited security authorization
- Sunset in 2022-2023 as DoD365/Teams matured
- CVR demonstrated the DoD's appetite for modern collaboration tools, and the speed at which they could be adopted when institutional barriers were lowered

---

## 4. Cloud Infrastructure

### 4.1 JWCC (Joint Warfighting Cloud Capability)

JWCC replaced the controversial JEDI (Joint Enterprise Defense Infrastructure) contract.

| Parameter | Detail |
|-----------|--------|
| Contract value | Up to $9 billion (ceiling) |
| Duration | Multi-year, indefinite-delivery/indefinite-quantity (IDIQ) |
| Vendors | Amazon Web Services (AWS), Microsoft Azure, Google Cloud, Oracle |
| Classification levels | IL2 through IL6+; TS/SCI cloud capabilities |
| Purpose | Provide enterprise cloud services to all DoD components |

**Relevance:**
- JWCC enables cloud-hosted communication and collaboration tools at multiple classification levels
- The multi-vendor approach avoids single-vendor lock-in
- Cloud infrastructure at IL5 and IL6 enables tools like Teams on SIPRNET
- However, cloud-dependent tools are useless to ships at sea with intermittent or no connectivity to cloud data centers

### 4.2 Zero Trust Strategy

The DoD released its Zero Trust Strategy in November 2022, with a target of achieving "Target Level" Zero Trust across the enterprise by FY2027.

Key pillars:
1. User identity (strong authentication, continuous verification)
2. Device health (compliant, patched, managed devices)
3. Network segmentation (microsegmentation, software-defined perimeters)
4. Application and workload security
5. Data protection (encryption, DLP, classification)
6. Visibility and analytics (SIEM, SOAR, behavioral analytics)
7. Automation and orchestration

**Implications for naval communication:**
- Zero Trust requires continuous network connectivity for authentication and policy enforcement
- Ships at sea with intermittent satellite links cannot maintain persistent Zero Trust connections
- Any naval communication platform must support a "disconnected Zero Trust" model, where authentication and authorization can function during periods of network isolation

---

## 5. Naval-Specific Programs

### 5.1 JADC2 (Joint All-Domain Command and Control)

JADC2 is the DoD's overarching concept for connecting sensors, shooters, and decision-makers across all domains (land, sea, air, space, cyber).

- Requires seamless, real-time data sharing across services and classification levels
- Navy's contribution is Project Overmatch

### 5.2 Project Overmatch

Project Overmatch is the US Navy's classified program to build the naval warfighting network of the future.

- Led by the Chief of Naval Operations (CNO) directly
- Aims to connect every Navy platform (ship, submarine, aircraft, unmanned system) into a mesh network
- Highly classified; limited public details
- Known objectives: low-latency targeting data, distributed maritime operations (DMO) support, resilient communications
- The program acknowledges that existing Navy communication systems are insufficient for peer-to-peer naval combat

### 5.3 CANES (Consolidated Afloat Networks and Enterprise Services)

CANES is the US Navy's shipboard network modernization program.

| Parameter | Detail |
|-----------|--------|
| Purpose | Replace and consolidate legacy shipboard networks into a single infrastructure |
| Legacy systems replaced | ISNS, ADNS, SCI networks, and others (previously 5+ separate networks per ship) |
| Contractor | Various (Leidos, General Dynamics, others) |
| Deployment | Rolling installation across the fleet; major surface combatants and carriers prioritized |
| Services | Shipboard email, web services, tactical applications, network management |
| Connectivity to shore | Via satellite links (military SATCOM, commercial SATCOM) |

**Limitations:**
- Satellite bandwidth to ships remains severely constrained (typically 2-8 Mbps for an entire carrier strike group)
- CANES provides the network; it does not solve the application-layer communication problem
- Teams/M365 are technically available on CANES, but bandwidth constraints make video calls impractical and even chat/email sluggish
- No offline-first design; applications assume persistent connectivity

---

## 6. The Critical Gap: Shipboard and Tactical Communication

### 6.1 Ships cannot effectively use Teams

Despite FLANK SPEED's 900,000-user footprint, ships at sea face:

- Satellite bandwidth of 2-8 Mbps shared among hundreds or thousands of personnel
- High latency (500-800 ms round-trip via GEO satellite)
- Intermittent connectivity (link drops during maneuvering, weather, EMCON conditions)
- Teams is designed for broadband, always-on connections; it degrades severely under these conditions
- Video conferencing is essentially unavailable at sea
- Even text chat and file sharing are unreliable

### 6.2 No offline messaging capability

No DoD-wide messaging platform supports offline message composition, queuing, and store-and-forward synchronization. When a ship loses its satellite link (which happens routinely), all cloud-based communication stops.

### 6.3 CAC authentication kills mobile usability

The Common Access Card (CAC) is the DoD's universal PKI credential. While essential for security, it creates severe usability problems on mobile devices:

- Physical CAC readers for smartphones are bulky and unreliable
- Derived Credentials (virtual CAC) exist but enrollment is complex and availability is inconsistent
- Many personnel simply do not attempt to use official tools on mobile, reverting instead to personal devices and commercial apps
- The result is a bifurcated communication environment: official (desktop, in office) and unofficial (mobile, everywhere else)

### 6.4 Cross-classification data transfer remains manual

Moving information between NIPRNET, SIPRNET, and JWICS requires:

- Cross-Domain Solutions (hardware appliances)
- Manual review and sanitization
- Significant time delays
- Personnel trained in CDS operations

This makes rapid, multi-classification communication impossible in practice. An operator with a SECRET-level intelligence product cannot share a relevant extract with an unclassified logistics system without a multi-step manual process.

### 6.5 Summary of the gap

The US military's communication gap is not about technology investment (which is enormous) but about the mismatch between enterprise IT architectures (designed for broadband, always-on, office-based environments) and the operational reality of naval forces (bandwidth-constrained, intermittently connected, mobile, multi-classification).

For documented security incidents related to US military communication, see [[security-breaches]].

---

## 7. Lessons for an Indian Naval Communication Platform

The US experience provides several critical lessons:

1. **Enterprise tools do not translate to shipboard environments.** Teams, M365, and cloud-based platforms fail at sea. Any Indian naval platform must be designed from the ground up for low-bandwidth, high-latency, intermittently connected operations.

2. **Authentication must be seamless on mobile.** CAC-based authentication has created a generation of DoD users who avoid official mobile tools. The Indian Navy must adopt biometric, certificate-based, or hardware-token authentication that does not require external readers.

3. **Offline-first architecture is non-negotiable.** Store-and-forward, local caching, and synchronization when connectivity resumes must be core design principles, not afterthoughts.

4. **Multi-vendor cloud is wise but insufficient.** JWCC's multi-vendor approach avoids lock-in, but cloud-dependent tools are useless without connectivity. Edge computing and local server instances on ships are essential.

5. **The Wickr shutdown demonstrates platform risk.** Dependence on a commercial vendor's product creates continuity risk. An indigenous, open-standard-based platform provides sovereignty and continuity.

6. **Cross-classification remains unsolved.** No nation has elegantly solved multi-classification communication. This represents both a challenge and an opportunity for innovative design.

---

## References

1. DoD Zero Trust Strategy (November 2022)
2. DISA JWCC Contract Announcements
3. Navy FLANK SPEED Program Documentation
4. DoD CIO Annual Reports
5. GAO Reports on DoD IT Modernization (GAO-23-106151, GAO-22-104208)
6. Congressional Research Service: "Defense Primer: Department of Defense Cloud Computing"
7. CANES Program Office Fact Sheets
8. JADC2 Implementation Plan (unclassified summary)
9. Defence News, C4ISRNET, Breaking Defense (open-source journalism)
10. AWS Wickr Sunset Announcement (2024)
