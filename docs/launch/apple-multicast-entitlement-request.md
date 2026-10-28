# Multicast networking entitlement request (iPhone app)

Form: <https://developer.apple.com/contact/request/networking-multicast>
(sign in as the account holder, team S6D533QSV2). Filed by the owner: an
agent cannot sign in to developer.apple.com (password + two-factor).

**App:** Vitals Monitor (app.vitals.ios, App Store id 6827931747)

**What does your app do?**

Vitals for iPhone shows the live readings (processor, memory, disks,
network, sensors, programs) of the user's own Windows PC or Mac running the
free Vitals desktop app, over the user's local network only. The app
refuses public addresses by design.

**Why does your app need multicast networking?**

To find the user's PCs on the local network without typing an IP address.
The desktop app advertises `_vitals._tcp` over Bonjour (mDNS, 224.0.0.251 /
ff02::fb, port 5353) only while the user has turned on Remote access. The
iPhone app already browses `_vitals._tcp` with `NWBrowser`, which works
without this entitlement on most networks.

We ask for the entitlement for the networks where Bonjour browsing does
not deliver: some mesh and enterprise Wi-Fi drop the unicast-response
path, and a PC that just woke from sleep re-announces only by multicast.
With the entitlement the app sends one mDNS query for `_vitals._tcp.local`
(the standard question, no other traffic) and listens for the PC's
announcement, so "Find nearby" works after the PC wakes.

**Traffic sent:** mDNS queries for `_vitals._tcp.local` only, on demand
when the user opens "Find nearby", stopped when the screen closes. No
other multicast or broadcast traffic, no scanning of other services, no
data sent to any server. Local network permission
(`NSLocalNetworkUsageDescription`) is requested before any of it.

**Privacy:** no account, no analytics, no tracking. App Privacy label:
Data Not Collected.

After approval: add an `entitlements` block with
`com.apple.developer.networking.multicast: true` to the Vitals target in
`apps/ios/project.yml` (automatic signing regenerates the profile), then
upload a new build (tracker S23-15).
