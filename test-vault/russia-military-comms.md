# Russia: Military Communication Systems Analysis

## Overview

Russia's military communication systems represent a study in contrasts: ambitious strategic programs and indigenous defence industry capabilities, set against catastrophic operational failures exposed during the invasion of Ukraine beginning in February 2022. The Russian Armed Forces' communication architecture reflects the broader structural problems of the Russian military, including top-down centralization, corruption-driven procurement shortfalls, poor maintenance, inadequate training, and a doctrinal rigidity that leaves forces unable to adapt when primary systems fail.

---

## 1. Core Network Infrastructure

### 1.1 ZSPD (Zakrytyi Segment Peredachi Dannykh / Closed Data Transmission Segment)

ZSPD is Russia's military intranet, conceptually equivalent to the US SIPRNET.

| Parameter | Detail |
|-----------|--------|
| Full name | Closed Data Transmission Segment of the Data Transfer Network |
| Classification | Secret and above |
| Connectivity | Fiber-optic backbone connecting major military districts, headquarters, and strategic installations |
| Services | Classified email, file transfer, command-and-control applications |
| Access | Dedicated terminals in secured facilities |
| Primary focus | Garrison and strategic (fixed installations) |

**Limitations:**
- ZSPD is fundamentally a garrison network; it was not designed for, and does not extend to, tactical or field environments
- Forward-deployed units, including naval vessels at sea, have limited or no ZSPD access
- The system relies on fixed infrastructure (fiber, microwave links) that is vulnerable to physical disruption
- User interfaces are reportedly dated, with limited usability for non-specialist personnel
- No mobile access component
- Integration with tactical communication systems is poor

### 1.2 ERA System and ERA Technopolis

ERA is a technology development and research center established by the Russian Ministry of Defence.

| Parameter | Detail |
|-----------|--------|
| Location | ERA Technopolis, Anapa, Krasnodar Krai |
| Established | 2018 |
| Purpose | Military technology research and development, including communication systems, AI, robotics, and cybersecurity |
| Staffing model | Conscript scientists and engineers serve their mandatory military service conducting R&D |
| Focus areas | Secure communication protocols, software-defined radios, electronic warfare, AI applications |

**Assessment:**
- ERA represents Russia's attempt to create a military technology innovation ecosystem
- Results have been mixed; the model of using conscript labour for advanced R&D has been questioned for effectiveness
- Specific communication products emerging from ERA are not well-documented in open sources
- ERA's impact on frontline communication capability during the Ukraine war appears to have been minimal

---

## 2. Defence Communication Industry

### 2.1 Voentelecom

Voentelecom (full name: Joint Stock Company Voentelecom) is the Russian Ministry of Defence's primary telecommunications operator.

- Manages and maintains military communication infrastructure
- Operates the fixed communication networks connecting military districts
- Provides satellite communication services for the armed forces
- Has been criticized for overcharging, underdelivering, and corruption
- Infrastructure maintained by Voentelecom has shown significant degradation in operational environments

### 2.2 Concern Sozvezdie (Constellation)

Concern Sozvezdie is a Russian defence electronics conglomerate focused on tactical communication systems.

- Part of Rostec state corporation
- Develops tactical radio systems, automated command-and-control systems, and electronic warfare equipment
- Products include the Andromeda-D unified tactical management system
- Andromeda-D is intended to provide digital C2 at the brigade and division level
- Deployment and operational effectiveness remain questionable; systems reportedly failed or were not available in sufficient quantities during Ukraine operations

### 2.3 Concern Avtomatika

Concern Avtomatika (part of Rostec) specializes in information security and secure communication.

- Develops encryption systems, secure communication terminals, and cybersecurity solutions
- Products include the M-500 and M-700 series encrypted communication systems
- Provides cryptographic protection for military and government communication channels

---

## 3. Tactical Radio Systems

### 3.1 Azart Encrypted Radios

The R-187P1 Azart is Russia's primary modern tactical encrypted radio.

| Parameter | Detail |
|-----------|--------|
| Manufacturer | Angstrem (Russian microelectronics company) |
| Type | Software-defined radio (SDR) |
| Bands | VHF, UHF |
| Encryption | GOST-certified cryptographic algorithms |
| Modes | Voice, data, frequency hopping |
| Weight | ~1.5 kg (handheld variant) |
| Range | 1.5-20 km (depending on variant and terrain) |

**Limitations exposed in Ukraine:**
- Insufficient quantities: the Russian military did not have enough Azart radios to equip all deploying units in February 2022
- Many units went to war with older, unencrypted or poorly encrypted radios (R-159, R-168 series)
- Units that lacked Azart radios communicated in the clear or on easily intercepted frequencies
- Open-source intelligence (OSINT) communities routinely intercepted Russian military radio traffic
- The production rate of Azart radios was insufficient to replace battlefield losses and equip mobilized reservists
- Sanctions on microelectronics (Angstrem relies on imported semiconductor components) have further constrained production

### 3.2 Chinese Baofeng Radios as Stopgap

One of the most remarkable developments of the Ukraine war has been the widespread use of Chinese-manufactured Baofeng commercial radios by Russian military units.

- Baofeng UV-5R and similar models cost $20-50 on the civilian market
- No encryption capability
- No frequency hopping
- Used by Russian units that lacked military-grade radios
- Communications on Baofeng radios were trivially intercepted by Ukrainian forces and OSINT analysts
- Baofeng usage was confirmed through captured equipment, intercepted transmissions, and photographs from the front
- The use of $25 commercial radios by a nuclear superpower's military became a symbol of Russia's procurement failures

---

## 4. Satellite Communication Systems

### 4.1 Meridian Constellation

| Parameter | Detail |
|-----------|--------|
| Type | Highly elliptical orbit (HEO) military communication satellites |
| Operator | Russian Ministry of Defence |
| Coverage | Arctic and high-latitude regions (complementing GEO satellites) |
| Purpose | Secure communication for strategic forces, including Northern Fleet submarines |
| Status | Constellation maintained but aging; replacement launches ongoing |

### 4.2 Blagovest Constellation

| Parameter | Detail |
|-----------|--------|
| Type | Geostationary (GEO) military communication satellites |
| Operator | Russian Ministry of Defence |
| Coverage | Russian territory and areas of military interest |
| Purpose | High-bandwidth military communication |
| Bands | Ka-band, Q-band (claimed) |
| Status | Four satellites launched (2017-2023); operational |

### 4.3 Raduga (Globus) Constellation

| Parameter | Detail |
|-----------|--------|
| Type | Geostationary military communication satellites |
| Operator | Russian Ministry of Defence |
| Purpose | Strategic communication, including nuclear command and control |
| Status | Legacy constellation; being supplemented by Blagovest |

### 4.4 Satellite Communication in Practice

Despite maintaining multiple satellite constellations, Russian forces in Ukraine demonstrated severe deficiencies in satellite communication:

- Satellite terminals were scarce at the tactical level
- Many units relied on civilian or captured communication infrastructure
- The Russian military's inability to provide reliable SATCOM to forward units contributed to the widespread use of commercial and unsecured communication methods

---

## 5. Ukraine War: Communication Failures

The Russian invasion of Ukraine beginning on February 24, 2022 exposed catastrophic failures in Russian military communication at every level.

### 5.1 Destruction of Own Cell Tower Infrastructure

In the opening phase of the invasion, Russian forces destroyed Ukrainian cellular infrastructure in occupied areas. This had an immediate and devastating unintended consequence:

- Russian military communication plans had, in several documented cases, relied on Ukrainian civilian cellular networks as a backup or primary communication channel
- Russian secure communication systems (ZSPD, Azart radios) were insufficient in quantity and coverage
- By destroying cell towers, Russian forces cut off their own fallback communication path
- Units were left unable to communicate with higher headquarters, adjacent units, or supporting elements

### 5.2 Open Radio Intercepts

Ukrainian signals intelligence, supplemented by Western intelligence sharing and open-source intelligence communities, intercepted vast quantities of Russian military radio traffic.

- Unencrypted voice communications were intercepted, recorded, and published online
- Intercepted calls revealed operational orders, unit locations, logistics failures, morale problems, and evidence of war crimes
- The scale of intercepts was unprecedented in modern conventional warfare
- Russian officers were heard discussing operations, casualties, and supply shortages on open channels
- These intercepts provided both intelligence value and strategic propaganda value to Ukraine

### 5.3 Telegram Usage

With secure military communication systems failing, Russian military personnel at all levels turned to Telegram, the Russian-origin (but UAE-based) commercial messaging application.

- Telegram was used for operational coordination, logistics requests, and even tactical commands
- Telegram groups were created for units, supply chains, and volunteer coordination
- While Telegram offers optional end-to-end encryption (Secret Chats), regular chats are encrypted only in transit, with Telegram holding the encryption keys
- Metadata (who communicates with whom, when, how often) was available to Telegram's servers
- Ukrainian intelligence exploited Telegram communications for targeting
- The irony: Russia's own military personnel used a platform that Telegram's founder (Pavel Durov) had created after being forced out of Russia, and that Russian authorities had previously attempted to ban

### 5.4 Generals Killed Due to Communication Failures

The most consequential result of Russia's communication failures was the killing of an extraordinary number of general officers in the early months of the war.

| General | Rank | Date Killed | Circumstances |
|---------|------|-------------|---------------|
| Andrei Sukhovetsky | Major General, VDV Deputy Commander | ~March 3, 2022 | Killed by sniper; position reportedly compromised via intercepted communications |
| Vitaly Gerasimov | Major General, 41st Combined Arms Army Chief of Staff | March 7, 2022 | Killed near Kharkiv; reportedly using unsecured phone after secure communication systems failed |
| Andrei Mordvichev | Major General | March 18, 2022 | Killed at Chornobaivka airfield; repeated Russian use of the same airfield (attributed to poor C2 communication) |
| Yakov Rezantsev | Lieutenant General | March 25, 2022 | Killed by Ukrainian strike; location reportedly identified through electronic signature |
| Andrei Simonov | Major General, Electronic Warfare | April 2022 | Killed by artillery strike on his command post; command post located via electronic emissions |
| Kanamat Botashev | Major General (retired, volunteer pilot) | May 22, 2022 | Shot down over Luhansk |

**Analysis:**
- Senior Russian officers were forced to move forward to personally direct operations because communication systems could not relay orders effectively through the chain of command
- Their presence in forward areas, combined with electronic emissions from command posts, made them targets
- The loss of multiple general officers in the first weeks of the war was historically unprecedented and directly attributable to communication system failures
- Western military analysts assessed that the Russian military's communication failures were a primary cause of its inability to execute coordinated combined-arms operations

### 5.5 Centralized C2 Doctrine vs. Mission Command

Russian military doctrine is built on centralized command and control, where decisions flow from the top down and subordinate commanders have limited authority to act independently (in contrast to Western "mission command" or "Auftragstaktik").

This doctrine requires reliable, high-bandwidth communication between senior commanders and subordinate units. When communication systems failed:

- Subordinate commanders lacked the training, authority, and doctrinal framework to make independent decisions
- Units froze in place, waiting for orders that never came
- Coordination between adjacent units collapsed
- Logistics convoys stopped moving because they could not receive route updates or waypoint changes
- Artillery could not coordinate with maneuver units for fire support

The communication failure thus amplified a doctrinal weakness: a military that cannot delegate authority requires perfect communication, and perfect communication does not exist in combat.

### 5.6 Wagner Group Mutiny: Communication Seams

The June 2023 Wagner Group mutiny (Prigozhin's "March of Justice") exposed another dimension of Russia's communication vulnerabilities.

- The Wagner Group operated on its own communication systems, separate from the Russian Ministry of Defence
- During the mutiny, the Russian military's ability to communicate with, monitor, or coordinate against Wagner forces was hampered by this separation
- Wagner's Telegram channels were used to broadcast Prigozhin's statements, recruit support, and coordinate the march on Moscow
- The Russian military's response was confused and delayed, partly because of the communication gap between official military channels and the parallel Wagner communication network
- The incident demonstrated that Russia's military communication architecture could not handle the challenge of a force that operated outside its communication hierarchy

---

## 6. Naval Communication Specifics

### 6.1 Russian Navy Communication Architecture

The Russian Navy operates communication systems that mirror the broader military's structure:

- Shore-to-ship: VLF/ELF (for submarine communication), HF, satellite (Meridian, Blagovest)
- Ship-to-ship: UHF, VHF, satellite relay
- Flagship systems: legacy Soviet-era communication suites upgraded with newer encryption modules
- Newer vessels (Gorshkov-class frigates, Borei-class submarines) have modern integrated communication suites, but the majority of the fleet operates older systems

### 6.2 Black Sea Fleet Communication Failures

The sinking of the cruiser Moskva (April 14, 2022) by Ukrainian Neptune anti-ship missiles highlighted communication and coordination failures:

- The Moskva was reportedly not using all available defensive systems at the time of the strike
- Communication between the Moskva and other fleet elements, as well as shore-based air defense, may have been degraded
- Subsequent Russian Black Sea Fleet operations showed improved caution but continued communication challenges

---

## 7. Lessons and Implications

### 7.1 Communication system failures are lethal

The Ukraine war proved that communication system failures directly cause casualties, including at the general officer level. This is not an abstract risk; it is a demonstrated, documented consequence.

### 7.2 Quantity matters as much as quality

Russia had modern encrypted radios (Azart), but not enough of them. A communication system that equips 30% of a force is not a communication system; it is an aspiration.

### 7.3 Personnel will use whatever works

When military systems fail, personnel will use Telegram, Baofeng radios, captured enemy equipment, or shouting. They will not accept non-communication. The question is whether the alternative they adopt is secure or insecure.

### 7.4 Centralized doctrine demands perfect communication

Any military that requires senior officers to approve tactical decisions must provide those officers with reliable, resilient communication. If it cannot, it must either fix its communication or change its doctrine.

### 7.5 Parallel force communication is a vulnerability

The Wagner mutiny showed that a military that cannot communicate with (or monitor) all of its forces creates exploitable seams.

### 7.6 Relevance to Indian naval communication

India's Navy does not face Russia's specific problems, but the underlying lessons apply universally:

- Communication systems must be available in sufficient quantity to reach every user who needs them
- Systems must be resilient to degraded conditions (low bandwidth, intermittent connectivity, electronic warfare)
- User experience matters: if the official system is harder to use than Telegram or WhatsApp, personnel will use the commercial alternative
- Offline capability and store-and-forward functionality are not luxury features; they are survival features

For documented security incidents related to Russian military communication, see [[security-breaches]].

---

## References

1. Royal United Services Institute (RUSI): "Preliminary Lessons in Conventional Warfighting from Russia's Invasion of Ukraine" (2022)
2. International Institute for Strategic Studies (IISS): "Russia's Military Communications" (2023)
3. Chatham House: "Russian Military Capability in a Ten-Year Perspective" (2023)
4. Center for Strategic and International Studies (CSIS): "Russian Military Communications Failures in Ukraine"
5. OSINT analyses of intercepted Russian military communications (multiple sources)
6. Congressional Research Service: "Russia's War in Ukraine: Military and Intelligence Aspects"
7. Janes Defence Intelligence: Russian military equipment profiles
8. Open-source journalism: The Economist, Financial Times, Der Spiegel, Bellingcat, Oryx
9. Telegram OSINT channels documenting Russian military communication equipment captures
10. US Department of Defense press briefings and background briefings on Russian military performance
