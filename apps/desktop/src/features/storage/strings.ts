/**
 * Disk storage translations.
 *
 * Own namespace, same reasoning as the other feature bundles. English and
 * Romanian side by side so a key cannot be added to one and forgotten in the
 * other.
 */

import { i18n } from '@vitals/i18n';

import { devBundles } from './devclean/strings';

export const STORAGE_NS = 'storage';

const en = {
  title: 'Storage',
  subtitle: 'Where the space on this computer went, and what could be reclaimed.',

  volumes: {
    heading: 'Drives',
    pick: 'Choose a drive to scan',
    // Distinct from `scan.rescan`. Both are buttons on this screen, and two
    // controls sharing an accessible name is ambiguous for a screen reader.
    refresh: 'Refresh drives',
    used: '{{used}} of {{total}} used',
    free: '{{free}} free',
    unknownCapacity: 'Capacity not reported',
    label: '{{mount}} {{label}}',
    scanHint: 'A scan reads every folder on the drive, several at once, and counts every file.',
    turbo: 'Turbo scan available',
    turboHint:
      'This drive is NTFS, so with administrator rights Vitals can read its file table directly and finish in seconds instead of minutes.',
    indexed: 'Saved index: a rescan reads only what changed',
    empty: 'No drives reported.',
  },

  kind: {
    hdd: 'Hard disk',
    ssd: 'SSD',
    nvme: 'NVMe SSD',
    removable: 'Removable',
    network: 'Network drive',
    optical: 'Optical drive',
    unknown: 'Unknown type',
  },

  scan: {
    start: 'Scan this drive',
    rescan: 'Scan again',
    turbo: 'Turbo scan (administrator)',
    turboHint:
      'Windows asks for approval once. Vitals then reads the drive’s file table directly, which takes seconds, and counts folders a normal scan cannot open.',
    full: 'Full scan',
    fullHint: 'Reads every folder again and ignores the saved index.',
    declined: 'Administrator approval was declined, so nothing was scanned.',
    declinedKept:
      'Administrator approval was declined, so nothing was scanned. The last result is still shown.',
    phase: {
      approval: 'Waiting for administrator approval…',
      reading: 'Reading the drive’s file table…',
      building: 'Adding up folder sizes…',
      journal: 'Checking what changed since the last scan…',
    },
    cancel: 'Stop the scan',
    running: 'Scanning {{root}}…',
    runningDetail: 'You can stop at any time and keep what was found.',
    progress: '{{files}} files · {{size}} · {{rate}} files a second',
    progressStarting: 'Starting…',
    now: 'Reading {{path}}',
    idleTitle: 'Nothing scanned yet',
    idleBody: 'Pick a drive above and start a scan to see which folders are using the space.',
    failed: 'The scan did not finish. {{message}}',
    stale: 'Showing the last completed scan. {{message}}',
  },

  result: {
    heading: 'Largest folders',
    total: '{{allocated}} on disk across {{files}} files',
    logical: '{{logical}} as file sizes',
    logicalHint:
      'Two figures because they measure different things. "On disk" is the space that would actually be freed: files are rounded up to whole clusters, and a file stored once but linked from several folders is counted once. "As file sizes" is the plain sum of file lengths, which is what File Explorer shows.',
    elapsed: 'Scanned in {{seconds}}s',
    turbo:
      'Read from the drive’s file table with administrator rights, including folders a normal scan cannot open.',
    incremental:
      'Folders reused from the saved index: {{reused}}; re-read because they changed: {{relisted}}.',
    dedup: 'Counted {{count}} linked files once, which kept {{size}} out of the total.',
    links_one: '{{n}} link to another folder was not followed.',
    links_other: '{{n}} links to other folders were not followed.',
    linksHint:
      'Shortcuts such as "My Documents" or "All Users" point at folders that are counted where they really are. Following them would count the same files twice, or never finish.',
    noCluster:
      'The cluster size of this drive could not be read, so files were not rounded up. The totals are slightly lower than the real on-disk figure.',
    cancelled: 'You stopped this scan, so every figure below is a lower bound.',
    incompleteScan_one: '{{count}} folder could not be read and is not included in the totals.',
    incompleteScan_other: '{{count}} folders could not be read and are not in the totals.',
    elevationWould_one: '{{count}} of them would probably be readable if Vitals ran as admin.',
    elevationWould_other: '{{count}} of them would probably be readable if Vitals ran as admin.',
    emptyTitle: 'No folders to show',
    emptyBody: 'The scan finished but found nothing under this root that takes measurable space.',
    filterEmpty: 'Nothing matches',
    filterEmptyBody: 'Clear the filter to see every folder from the scan.',
  },

  search: 'Filter folders by path',
  clear: 'Clear the filter',
  sortLabel: 'Sort folders by',
  sort: {
    allocated: 'On disk',
    logical: 'File sizes',
    files: 'Files',
    path: 'Path',
  },

  column: {
    path: 'Folder',
    allocated: 'On disk',
    logical: 'File sizes',
    files: 'Files',
  },

  incomplete: 'Incomplete',
  incompleteHint: 'This folder could not be fully read: {{reason}}. Its size is a lower bound.',

  skip: {
    accessDenied: 'permission denied',
    reparsePoint: 'it is a link to somewhere else, which was not followed',
    cancelled: 'the scan was stopped before it got there',
    vanished: 'it was removed while the scan was running',
    osError: 'Windows refused to open it',
  },

  cleanup: {
    heading: 'Reclaimable space',
    scan: 'Look for reclaimable space',
    rescan: 'Check again',
    running: 'Measuring caches and temporary files…',
    cancel: 'Stop checking',
    failed: 'Could not check for reclaimable space. {{message}}',
    total: 'About {{size}} looks reclaimable',
    totalFloor: 'At least {{size}} looks reclaimable',
    totalScope:
      'Counting only the safe and review-first items — risky ones are listed but not added up.',
    unmeasured_one: '{{count}} location could not be measured, so the real figure is higher.',
    unmeasured_other: '{{count}} locations could not be measured, so the real figure is higher.',
    idleTitle: 'Not checked yet',
    idleBody:
      'Vitals can measure the caches and temporary folders that Windows and applications leave behind.',
    emptyTitle: 'Nothing to reclaim',
    emptyBody:
      'None of the usual cache and temporary locations exist or hold anything on this machine.',

    delete: 'Delete',
    deleteUnavailable:
      'Not from here. This is not space Windows manages, so no Windows tool empties it. Add its folders to review in Explore and send them to the Recycle Bin.',
    free: {
      diskCleanup: 'Clean up…',
      componentCleanup: 'Clean up…',
      hibernateOff: 'Turn off…',
    },
    unknownSize: 'Not measured',
    reasonNeedsElevation:
      'This location exists but cannot be read without administrator rights, so its size is unknown — not zero.',
    reasonUnreadable:
      'This location exists but could not be measured. Its size is unknown, which is not the same as empty.',
    needsElevation: 'Needs admin',
  },

  safety: {
    safe: 'Safe to remove',
    safeBody: 'Regenerated automatically. Removing these costs nothing but a slower next start.',
    review: 'Worth reviewing',
    reviewBody:
      'Recoverable, but you pay for it: signed-out browser sessions, updates that get re-downloaded.',
    risky: 'Risky',
    riskyBody:
      'These change how the system behaves or hold the only copy of something. Listed because the space is real, but they should be handled through their own Windows setting.',
  },

  kindLabel: {
    userTemp: 'Temporary files (your account)',
    systemTemp: 'Temporary files (system)',
    browserCache: 'Browser cache',
    windowsUpdateCache: 'Windows Update cache',
    recycleBin: 'Recycle Bin',
    crashDump: 'Crash dumps',
    previousWindows: 'Previous Windows installation',
    hibernation: 'Hibernation file',
    packageManagerCache: 'Package manager cache',
    thumbnailCache: 'Thumbnail cache',
    deliveryOptimisation: 'Delivery Optimisation cache',
    componentStore: 'Windows component store',
  },

  kindReason: {
    userTemp:
      'Temporary files written by applications. Windows never clears these once the writing program has exited.',
    systemTemp:
      'System temporary files, mostly installer scratch space left behind by setup programs.',
    browserCache:
      'Cached web content. Removing it frees space immediately and costs only a slower first load of sites you have visited.',
    windowsUpdateCache:
      'Update packages that are already installed. Windows keeps them for repairs and re-downloads them if needed.',
    recycleBin:
      'Files you deleted but have not purged. Emptying this is the point at which deletion becomes permanent.',
    crashDump:
      'Memory dumps written after a crash. Useful only while a fault is being diagnosed; a full dump can be as large as your RAM.',
    previousWindows:
      'The previous Windows installation kept after a feature update. It is the only way back to the old build, and Windows removes it automatically after ten days.',
    hibernation:
      'The hibernation image, sized to a fraction of your RAM. It cannot be deleted as a file — turn hibernation off with powercfg /h off instead, which also disables fast startup.',
    packageManagerCache:
      'Downloaded package archives kept by a developer tool. Rebuilt on demand at the cost of re-downloading.',
    thumbnailCache: "Explorer's thumbnail and icon database. Regenerated as you browse folders.",
    deliveryOptimisation:
      'Update fragments cached for sharing with other machines on your network. Purely a bandwidth optimisation.',
    componentStore:
      'Old versions of Windows components kept after updates. Only Windows can tell which are no longer needed, so its size is not measured here — never delete this folder by hand.',
  },

  consequence: {
    recycleBin:
      'This empties the Recycle Bin on every drive. Everything in it is then gone for good and cannot be restored.',
    previousWindows:
      'This removes the previous Windows installation. You will no longer be able to go back to the Windows version you had before the last feature update.',
    hibernation:
      'This turns hibernation off. The computer can no longer hibernate, and fast startup is turned off too, so it may start more slowly. You can turn it back on later with powercfg /h on.',
  },

  windows: {
    confirmTitle: 'Free {{label}} with Windows?',
    reportTitle: '{{label}}: what Windows freed',
    notRunTitle: '{{label}}: nothing was run',
    tool: {
      diskCleanup:
        'Windows Disk Cleanup runs with only this item ticked. Vitals deletes nothing itself.',
      componentCleanup:
        'Windows removes the component versions it no longer needs (DISM component cleanup). Installed updates stay uninstallable. This can take several minutes.',
      hibernateOff:
        'Windows turns hibernation off with powercfg, and removes the hibernation file itself.',
    },
    confirm: {
      diskCleanup: 'Run Disk Cleanup',
      componentCleanup: 'Run component cleanup',
      hibernateOff: 'Turn hibernation off',
    },
    elevation:
      'Windows will ask for administrator approval once, for this action only. Vitals itself keeps running without administrator rights.',
    understood: 'I understand this cannot be undone',
    running: 'Windows is working…',
    stage: {
      measuring: 'Measuring what is there now…',
      approval: 'Waiting for administrator approval…',
      running: 'Windows is cleaning up…',
      remeasuring: 'Measuring what is left…',
    },
    elapsed: '{{seconds}} s',
    soFar: '{{size}} freed on the drive so far',
    outcome: {
      done: 'Done',
      needsRestart: 'Restart to finish',
      toolFailed: 'Windows reported a problem',
    },
    freed: '{{size}} freed',
    freedDrive: '{{size}} more free on the drive',
    freedUnknown: 'How much was freed could not be measured',
    unchangedBadge: 'Nothing changed',
    unchanged:
      'Windows finished without an error, but nothing measurable was freed here. Windows decides what it removes; Vitals does not remove anything in its place.',
    needsRestart: 'Windows finishes this the next time the computer restarts.',
    toolFailed:
      'The Windows tool ended with code {{code}}. Whatever it managed to free is counted above.',
    location: 'This location',
    beforeAfter: '{{before}} before, {{after}} after',
    drive: 'The drive',
    driveGained: '{{size}} more free space',
    driveLost: '{{size}} less free space (something else wrote to the drive meanwhile)',
    measuredNote: 'Measured before and after Windows ran, not estimated.',
    declined: 'Nothing was cleaned. {{message}}',
    failed: 'Nothing was cleaned. {{message}}',
    cancel: 'Cancel',
    close: 'Close',
    done: 'Done',
  },

  noHost: {
    title: 'No readings are arriving',
    body: 'Vitals cannot reach the part of itself that reads the disk. Nothing is wrong with the machine — restarting Vitals usually fixes this.',
  },

  explore: {
    heading: 'Explore',
    views: 'How to show the folders',
    view: {
      icicle: 'Layers',
      treemap: 'Blocks',
      list: 'List',
      largest: 'Largest folders',
      files: 'Largest files',
    },
    breadcrumb: 'Where you are',
    up: 'Up one level',
    open: 'Open {{name}}',
    ownFiles_one: '{{n}} file directly in this folder',
    ownFiles_other: '{{n}} files directly in this folder',
    smaller_one: '{{n}} smaller folder',
    smaller_other: '{{n}} smaller folders',
    share: '{{percent}} of {{parent}}',
    released:
      'This scan is no longer in memory, so it cannot be explored. Vitals lets go of it after fifteen minutes unused. Scan again to explore.',
    loading: 'Laying out…',
    failed: 'Could not show this folder. {{message}}',
    mapLabel:
      'Map of {{path}}. Each block is a folder, sized by the space it uses. Use the list below to move with the keyboard.',
    emptyFolder: 'This folder has no subfolders. Its files are counted in the total above.',
    menu: {
      open: 'Open here',
      reveal: 'Show in File Explorer',
      copy: 'Copy path',
    },
    filesEmpty: 'No files were large enough to list.',
  },

  basket: {
    heading: 'To review',
    hint: 'Add folders or files with + to review them here before sending them to the Recycle Bin.',
    add: 'Add to review',
    addItem: 'Add {{name}} to review',
    removeItem: 'Remove {{name}} from review',
    inBasket: 'In review — click to take it out',
    inBasketRemove: 'Remove from review',
    mapHint: 'right-click to add to review',
    mapInBasket: 'in review; right-click to take it out',
    summary_one: '{{n}} item in review · {{size}}',
    summary_other: '{{n}} items in review · {{size}}',
    review: 'Review…',
    clear: 'Clear',
    confirmTitle: 'Send to the Recycle Bin?',
    confirmBody:
      'These go to the Recycle Bin, not away for good: you can put any of them back from there until it is emptied. Windows folders and the folders your account is made of are refused.',
    confirm_one: 'Recycle {{n}} item · {{size}}',
    confirm_other: 'Recycle {{n}} items · {{size}}',
    recycling: 'Sending to the Recycle Bin…',
    cancel: 'Cancel',
    close: 'Close',
    done: 'Done',
    remove: 'Take {{name}} out of review',
    reportTitle: 'What happened',
    reportAll_one: '{{count}} item is in the Recycle Bin.',
    reportAll_other: 'All {{count}} items are in the Recycle Bin.',
    reportSome_one:
      '{{count}} item is in the Recycle Bin; {{left}} could not be moved and stayed where it was.',
    reportSome_other:
      '{{count}} items are in the Recycle Bin; {{left}} could not be moved and stayed where they were.',
    reportNone: 'Nothing was moved. Every item is still where it was; each line below says why.',
    failed: 'Nothing was moved. {{message}}',
  },

  outcome: {
    recycled: 'In the Recycle Bin',
    refused: 'Refused',
    wouldBePermanent: 'Not moved',
    missing: 'Already gone',
    locked: 'In use',
    accessDenied: 'Not allowed',
    failed: 'Failed',
  },

  outcomeWhy: {
    wouldBePermanent:
      'The Recycle Bin cannot hold this (it is too large, or its drive has no bin), so Windows would have deleted it for good. Vitals left it where it is.',
    missing: 'Nothing was there any more, so there was nothing to move.',
    accessDenied:
      'Windows did not allow Vitals to move it. It may belong to another account or need administrator rights.',
    failed: 'Windows could not move it (error {{code}}). It is still where it was.',
  },

  protection: {
    invalid: 'This is not a path Vitals can check safely, so it was not touched.',
    driveRoot: 'A whole drive cannot be sent to the Recycle Bin.',
    systemFolder:
      'This is part of Windows or an installed program. Removing it would break it; uninstall programs from the Apps screen instead.',
    userFolder:
      'This is one of the folders your account is made of. Its contents can be reviewed, but the folder itself stays.',
    systemFile: 'Windows marks this as a system file, so it is not moved.',
    noRecycleBin:
      'This drive has no Recycle Bin, so removing anything there would be permanent. Vitals does not do that.',
    vitals: 'This is where Vitals itself is installed.',
  },

  holders: {
    intro_one: 'This program has it open:',
    intro_other: 'These programs have it open:',
    pid: 'process {{pid}}',
    service: 'service {{name}}',
    unnamed: 'A program without a name',
    critical: 'Needs a restart',
    advice: 'Close it (or save and close the file in it), then try again.',
    none: 'Windows says no program has it open now. It may have been released a moment ago; try again.',
    unknown: 'It is in use, but Windows could not say by which program.',
  },
} as const;

const ro = {
  title: 'Stocare',
  subtitle: 'Unde s-a dus spațiul de pe acest calculator și ce se poate recupera.',

  volumes: {
    heading: 'Unități',
    pick: 'Alege o unitate de scanat',
    refresh: 'Reîmprospătează unitățile',
    used: '{{used}} din {{total}} folosiți',
    free: '{{free}} liberi',
    unknownCapacity: 'Capacitatea nu este raportată',
    label: '{{mount}} {{label}}',
    scanHint:
      'O scanare citește fiecare folder de pe unitate, mai multe deodată, și numără fiecare fișier.',
    turbo: 'Scanare Turbo disponibilă',
    turboHint:
      'Această unitate este NTFS, așa că, având drepturi de administrator, Vitals îi poate citi direct tabelul de fișiere și termină în câteva secunde în loc de minute.',
    indexed: 'Index salvat: o nouă scanare citește doar ce s-a schimbat',
    empty: 'Nu au fost raportate unități.',
  },

  kind: {
    hdd: 'Hard disk',
    ssd: 'SSD',
    nvme: 'SSD NVMe',
    removable: 'Detașabil',
    network: 'Unitate de rețea',
    optical: 'Unitate optică',
    unknown: 'Tip necunoscut',
  },

  scan: {
    start: 'Scanează această unitate',
    rescan: 'Scanează din nou',
    turbo: 'Scanare Turbo (administrator)',
    turboHint:
      'Windows cere aprobarea o singură dată. Apoi Vitals citește direct tabelul de fișiere al unității, în câteva secunde, și numără și folderele pe care o scanare obișnuită nu le poate deschide.',
    full: 'Scanare completă',
    fullHint: 'Citește din nou fiecare folder și ignoră indexul salvat.',
    declined: 'Aprobarea de administrator a fost refuzată, deci nu s-a scanat nimic.',
    declinedKept:
      'Aprobarea de administrator a fost refuzată, deci nu s-a scanat nimic. Rezultatul anterior este afișat în continuare.',
    phase: {
      approval: 'Se așteaptă aprobarea de administrator…',
      reading: 'Se citește tabelul de fișiere al unității…',
      building: 'Se adună dimensiunile folderelor…',
      journal: 'Se verifică ce s-a schimbat de la ultima scanare…',
    },
    cancel: 'Oprește scanarea',
    running: 'Se scanează {{root}}…',
    runningDetail: 'Poți opri oricând și păstrezi ce s-a găsit până atunci.',
    progress: '{{files}} fișiere · {{size}} · {{rate}} fișiere pe secundă',
    progressStarting: 'Se pornește…',
    now: 'Se citește {{path}}',
    idleTitle: 'Nu s-a scanat încă nimic',
    idleBody: 'Alege o unitate mai sus și pornește o scanare ca să vezi ce foldere ocupă spațiul.',
    failed: 'Scanarea nu s-a terminat. {{message}}',
    stale: 'Se afișează ultima scanare finalizată. {{message}}',
  },

  result: {
    heading: 'Cele mai mari foldere',
    total: '{{allocated}} pe disc, în {{files}} fișiere',
    logical: '{{logical}} ca dimensiuni de fișiere',
    logicalHint:
      'Două cifre pentru că măsoară lucruri diferite. „Pe disc” este spațiul care s-ar elibera efectiv: fișierele sunt rotunjite la clustere întregi, iar un fișier stocat o singură dată dar legat din mai multe foldere este numărat o dată. „Ca dimensiuni de fișiere” este suma simplă a lungimilor, adică exact ce arată File Explorer.',
    elapsed: 'Scanat în {{seconds}} s',
    turbo:
      'Citit din tabelul de fișiere al unității cu drepturi de administrator, inclusiv folderele pe care o scanare obișnuită nu le poate deschide.',
    incremental:
      'Foldere refolosite din indexul salvat: {{reused}}; recitite pentru că s-au schimbat: {{relisted}}.',
    dedup:
      'S-au numărat o singură dată {{count}} fișiere legate, ceea ce a scos {{size}} din total.',
    links_one: '{{n}} legătură către alt folder nu a fost urmată.',
    links_few: '{{n}} legături către alte foldere nu au fost urmate.',
    links_other: '{{n}} de legături către alte foldere nu au fost urmate.',
    linksHint:
      'Scurtături precum „My Documents” sau „All Users” duc la foldere care sunt numărate acolo unde se află de fapt. Urmarea lor ar număra aceleași fișiere de două ori sau nu s-ar termina niciodată.',
    noCluster:
      'Dimensiunea clusterului acestei unități nu a putut fi citită, deci fișierele nu au fost rotunjite. Totalurile sunt puțin mai mici decât cifra reală de pe disc.',
    cancelled: 'Ai oprit această scanare, deci fiecare cifră de mai jos este o limită inferioară.',
    incompleteScan_one: '{{count}} folder nu a putut fi citit și nu este inclus în totaluri.',
    incompleteScan_few: '{{count}} foldere nu au putut fi citite și nu sunt în totaluri.',
    incompleteScan_other: '{{count}} de foldere nu au putut fi citite și nu sunt în totaluri.',
    elevationWould_one:
      '{{count}} dintre ele ar fi probabil citibil dacă Vitals ar rula ca administrator.',
    elevationWould_few:
      '{{count}} dintre ele ar fi probabil citibile dacă Vitals ar rula ca administrator.',
    elevationWould_other:
      '{{count}} dintre ele ar fi probabil citibile dacă Vitals ar rula ca administrator.',
    emptyTitle: 'Nu sunt foldere de afișat',
    emptyBody:
      'Scanarea s-a terminat, dar nu a găsit nimic sub această rădăcină care să ocupe spațiu măsurabil.',
    filterEmpty: 'Nimic nu se potrivește',
    filterEmptyBody: 'Șterge filtrul ca să vezi toate folderele din scanare.',
  },

  search: 'Filtrează folderele după cale',
  clear: 'Șterge filtrul',
  sortLabel: 'Sortează folderele după',
  sort: {
    allocated: 'Pe disc',
    logical: 'Dimensiuni fișiere',
    files: 'Fișiere',
    path: 'Cale',
  },

  column: {
    path: 'Folder',
    allocated: 'Pe disc',
    logical: 'Dimensiuni fișiere',
    files: 'Fișiere',
  },

  incomplete: 'Incomplet',
  incompleteHint:
    'Acest folder nu a putut fi citit complet: {{reason}}. Dimensiunea lui este o limită inferioară.',

  skip: {
    accessDenied: 'permisiune refuzată',
    reparsePoint: 'este o legătură către altundeva, care nu a fost urmată',
    cancelled: 'scanarea a fost oprită înainte să ajungă acolo',
    vanished: 'a fost șters în timpul scanării',
    osError: 'Windows a refuzat să îl deschidă',
  },

  cleanup: {
    heading: 'Spațiu recuperabil',
    scan: 'Caută spațiu recuperabil',
    rescan: 'Verifică din nou',
    running: 'Se măsoară cache-urile și fișierele temporare…',
    cancel: 'Oprește verificarea',
    failed: 'Nu s-a putut verifica spațiul recuperabil. {{message}}',
    total: 'Aproximativ {{size}} par recuperabili',
    totalFloor: 'Cel puțin {{size}} par recuperabili',
    totalScope:
      'Se numără doar elementele sigure și cele de verificat — cele riscante sunt listate, dar nu adunate.',
    unmeasured_one: '{{count}} locație nu a putut fi măsurată, deci cifra reală este mai mare.',
    unmeasured_few: '{{count}} locații nu au putut fi măsurate, deci cifra reală este mai mare.',
    unmeasured_other:
      '{{count}} de locații nu au putut fi măsurate, deci cifra reală este mai mare.',
    idleTitle: 'Nu s-a verificat încă',
    idleBody:
      'Vitals poate măsura cache-urile și folderele temporare pe care Windows și aplicațiile le lasă în urmă.',
    emptyTitle: 'Nimic de recuperat',
    emptyBody:
      'Niciuna dintre locațiile obișnuite de cache și fișiere temporare nu există sau nu conține nimic pe această mașină.',

    delete: 'Șterge',
    deleteUnavailable:
      'Nu de aici. Acesta nu este spațiu gestionat de Windows, deci nicio unealtă Windows nu îl golește. Adaugă folderele lui la verificare în Explorează și trimite-le în Coșul de reciclare.',
    free: {
      diskCleanup: 'Curăță…',
      componentCleanup: 'Curăță…',
      hibernateOff: 'Dezactivează…',
    },
    unknownSize: 'Nemăsurat',
    reasonNeedsElevation:
      'Această locație există, dar nu poate fi citită fără drepturi de administrator, deci dimensiunea ei este necunoscută — nu zero.',
    reasonUnreadable:
      'Această locație există, dar nu a putut fi măsurată. Dimensiunea ei este necunoscută, ceea ce nu înseamnă goală.',
    needsElevation: 'Necesită administrator',
  },

  safety: {
    safe: 'Sigur de șters',
    safeBody:
      'Se regenerează automat. Ștergerea lor nu costă nimic în afară de o pornire mai lentă data viitoare.',
    review: 'Merită verificat',
    reviewBody:
      'Recuperabil, dar cu un cost: sesiuni de browser deconectate, actualizări descărcate din nou.',
    risky: 'Riscant',
    riskyBody:
      'Acestea schimbă comportamentul sistemului sau conțin singura copie a ceva. Sunt listate pentru că spațiul este real, dar ar trebui tratate din setarea lor proprie din Windows.',
  },

  kindLabel: {
    userTemp: 'Fișiere temporare (contul tău)',
    systemTemp: 'Fișiere temporare (sistem)',
    browserCache: 'Cache browser',
    windowsUpdateCache: 'Cache Windows Update',
    recycleBin: 'Coș de reciclare',
    crashDump: 'Fișiere de eroare (crash dumps)',
    previousWindows: 'Instalarea Windows anterioară',
    hibernation: 'Fișier de hibernare',
    packageManagerCache: 'Cache manager de pachete',
    thumbnailCache: 'Cache miniaturi',
    deliveryOptimisation: 'Cache Delivery Optimisation',
    componentStore: 'Depozitul de componente Windows',
  },

  kindReason: {
    userTemp:
      'Fișiere temporare scrise de aplicații. Windows nu le șterge niciodată după ce programul care le-a scris s-a închis.',
    systemTemp:
      'Fișiere temporare de sistem, în mare parte spațiu de lucru lăsat în urmă de programe de instalare.',
    browserCache:
      'Conținut web salvat local. Ștergerea eliberează spațiu imediat și costă doar o încărcare mai lentă a site-urilor deja vizitate.',
    windowsUpdateCache:
      'Pachete de actualizare deja instalate. Windows le păstrează pentru reparații și le descarcă din nou dacă este nevoie.',
    recycleBin:
      'Fișiere pe care le-ai șters, dar nu le-ai golit. Golirea este momentul în care ștergerea devine permanentă.',
    crashDump:
      'Copii ale memoriei scrise după o eroare. Utile doar cât timp se investighează o defecțiune; un dump complet poate fi cât toată memoria RAM.',
    previousWindows:
      'Instalarea Windows anterioară, păstrată după o actualizare majoră. Este singura cale de întoarcere la versiunea veche, iar Windows o șterge automat după zece zile.',
    hibernation:
      'Imaginea de hibernare, dimensionată la o fracțiune din memoria RAM. Nu poate fi ștearsă ca fișier — dezactivează hibernarea cu powercfg /h off, ceea ce oprește și pornirea rapidă.',
    packageManagerCache:
      'Arhive de pachete descărcate, păstrate de o unealtă de dezvoltare. Se reconstruiesc la cerere, cu prețul unei noi descărcări.',
    thumbnailCache:
      'Baza de date cu miniaturi și pictograme a Explorer. Se regenerează pe măsură ce navighezi prin foldere.',
    deliveryOptimisation:
      'Fragmente de actualizări păstrate pentru partajarea cu alte calculatoare din rețea. Este strict o optimizare de lățime de bandă.',
    componentStore:
      'Versiuni vechi ale componentelor Windows, păstrate după actualizări. Doar Windows știe care nu mai sunt necesare, așa că dimensiunea nu se măsoară aici — nu șterge niciodată acest folder de mână.',
  },

  consequence: {
    recycleBin:
      'Aceasta golește Coșul de reciclare de pe toate unitățile. Tot ce este în el dispare definitiv și nu mai poate fi restaurat.',
    previousWindows:
      'Aceasta șterge instalarea Windows anterioară. Nu te vei mai putea întoarce la versiunea de Windows pe care o aveai înainte de ultima actualizare majoră.',
    hibernation:
      'Aceasta dezactivează hibernarea. Calculatorul nu mai poate hiberna, iar pornirea rapidă se oprește și ea, deci pornirea poate fi mai lentă. O poți reactiva mai târziu cu powercfg /h on.',
  },

  windows: {
    confirmTitle: 'Eliberezi {{label}} cu Windows?',
    reportTitle: '{{label}}: ce a eliberat Windows',
    notRunTitle: '{{label}}: nu s-a rulat nimic',
    tool: {
      diskCleanup:
        'Curățarea discului din Windows rulează doar cu acest element bifat. Vitals nu șterge nimic singur.',
      componentCleanup:
        'Windows șterge versiunile de componente de care nu mai are nevoie (curățarea componentelor DISM). Actualizările instalate pot fi dezinstalate în continuare. Poate dura câteva minute.',
      hibernateOff:
        'Windows dezactivează hibernarea cu powercfg și șterge singur fișierul de hibernare.',
    },
    confirm: {
      diskCleanup: 'Pornește Curățarea discului',
      componentCleanup: 'Pornește curățarea componentelor',
      hibernateOff: 'Dezactivează hibernarea',
    },
    elevation:
      'Windows va cere o singură dată aprobarea de administrator, doar pentru această acțiune. Vitals rulează în continuare fără drepturi de administrator.',
    understood: 'Înțeleg că aceasta nu poate fi anulată',
    running: 'Windows lucrează…',
    stage: {
      measuring: 'Se măsoară ce există acum…',
      approval: 'Se așteaptă aprobarea de administrator…',
      running: 'Windows face curățenie…',
      remeasuring: 'Se măsoară ce a rămas…',
    },
    elapsed: '{{seconds}} s',
    soFar: '{{size}} eliberați pe unitate până acum',
    outcome: {
      done: 'Gata',
      needsRestart: 'Repornește pentru a termina',
      toolFailed: 'Windows a raportat o problemă',
    },
    freed: '{{size}} eliberați',
    freedDrive: '{{size}} în plus liberi pe unitate',
    freedUnknown: 'Nu s-a putut măsura cât s-a eliberat',
    unchangedBadge: 'Nimic schimbat',
    unchanged:
      'Windows a terminat fără eroare, dar aici nu s-a eliberat nimic măsurabil. Windows decide ce șterge; Vitals nu șterge nimic în locul lui.',
    needsRestart: 'Windows termină la următoarea repornire a calculatorului.',
    toolFailed:
      'Unealta Windows s-a încheiat cu codul {{code}}. Ce a reușit să elibereze este numărat mai sus.',
    location: 'Această locație',
    beforeAfter: '{{before}} înainte, {{after}} după',
    drive: 'Unitatea',
    driveGained: '{{size}} spațiu liber în plus',
    driveLost: '{{size}} spațiu liber în minus (altceva a scris pe unitate între timp)',
    measuredNote: 'Măsurat înainte și după rularea Windows, nu estimat.',
    declined: 'Nu s-a curățat nimic. {{message}}',
    failed: 'Nu s-a curățat nimic. {{message}}',
    cancel: 'Anulează',
    close: 'Închide',
    done: 'Gata',
  },

  noHost: {
    title: 'Nu sosesc măsurători',
    body: 'Vitals nu poate ajunge la partea din el care citește discul. Nu este nimic în neregulă cu mașina — de obicei repornirea aplicației Vitals rezolvă asta.',
  },

  explore: {
    heading: 'Explorează',
    views: 'Cum să fie afișate folderele',
    view: {
      icicle: 'Straturi',
      treemap: 'Blocuri',
      list: 'Listă',
      largest: 'Cele mai mari foldere',
      files: 'Cele mai mari fișiere',
    },
    breadcrumb: 'Unde te afli',
    up: 'Un nivel mai sus',
    open: 'Deschide {{name}}',
    ownFiles_one: '{{n}} fișier direct în acest folder',
    ownFiles_few: '{{n}} fișiere direct în acest folder',
    ownFiles_other: '{{n}} de fișiere direct în acest folder',
    smaller_one: '{{n}} folder mai mic',
    smaller_few: '{{n}} foldere mai mici',
    smaller_other: '{{n}} de foldere mai mici',
    share: '{{percent}} din {{parent}}',
    released:
      'Această scanare nu mai este în memorie, așa că nu poate fi explorată. Vitals o eliberează după cincisprezece minute de nefolosire. Scanează din nou pentru a explora.',
    loading: 'Se așază…',
    failed: 'Folderul nu poate fi afișat. {{message}}',
    mapLabel:
      'Harta pentru {{path}}. Fiecare bloc este un folder, dimensionat după spațiul pe care îl ocupă. Folosește lista de mai jos pentru a naviga cu tastatura.',
    emptyFolder:
      'Acest folder nu are subfoldere. Fișierele lui sunt incluse în totalul de mai sus.',
    menu: {
      open: 'Deschide aici',
      reveal: 'Arată în File Explorer',
      copy: 'Copiază calea',
    },
    filesEmpty: 'Niciun fișier nu a fost destul de mare pentru a fi listat.',
  },

  basket: {
    heading: 'De verificat',
    hint: 'Adaugă foldere sau fișiere cu + ca să le verifici aici înainte de a le trimite în Coșul de reciclare.',
    add: 'Adaugă la verificare',
    addItem: 'Adaugă {{name}} la verificare',
    removeItem: 'Scoate {{name}} de la verificare',
    inBasket: 'La verificare — apasă ca să îl scoți',
    inBasketRemove: 'Scoate de la verificare',
    mapHint: 'clic dreapta pentru a adăuga la verificare',
    mapInBasket: 'la verificare; clic dreapta ca să îl scoți',
    summary_one: '{{n}} element la verificare · {{size}}',
    summary_few: '{{n}} elemente la verificare · {{size}}',
    summary_other: '{{n}} de elemente la verificare · {{size}}',
    review: 'Verifică…',
    clear: 'Golește lista',
    confirmTitle: 'Trimiți în Coșul de reciclare?',
    confirmBody:
      'Acestea ajung în Coșul de reciclare, nu dispar definitiv: poți readuce oricare dintre ele de acolo până când coșul este golit. Folderele Windows și cele din care este făcut contul tău sunt refuzate.',
    confirm_one: 'Reciclează {{n}} element · {{size}}',
    confirm_few: 'Reciclează {{n}} elemente · {{size}}',
    confirm_other: 'Reciclează {{n}} de elemente · {{size}}',
    recycling: 'Se trimit în Coșul de reciclare…',
    cancel: 'Renunță',
    close: 'Închide',
    done: 'Gata',
    remove: 'Scoate {{name}} de la verificare',
    reportTitle: 'Ce s-a întâmplat',
    reportAll_one: '{{count}} element este în Coșul de reciclare.',
    reportAll_few: 'Toate cele {{count}} elemente sunt în Coșul de reciclare.',
    reportAll_other: 'Toate cele {{count}} de elemente sunt în Coșul de reciclare.',
    reportSome_one:
      '{{count}} element este în Coșul de reciclare; {{left}} nu au putut fi mutate și au rămas pe loc.',
    reportSome_few:
      '{{count}} elemente sunt în Coșul de reciclare; {{left}} nu au putut fi mutate și au rămas pe loc.',
    reportSome_other:
      '{{count}} de elemente sunt în Coșul de reciclare; {{left}} nu au putut fi mutate și au rămas pe loc.',
    reportNone:
      'Nu s-a mutat nimic. Fiecare element este tot acolo unde era; fiecare rând de mai jos spune de ce.',
    failed: 'Nu s-a mutat nimic. {{message}}',
  },

  outcome: {
    recycled: 'În Coșul de reciclare',
    refused: 'Refuzat',
    wouldBePermanent: 'Nemutat',
    missing: 'Deja dispărut',
    locked: 'În folosință',
    accessDenied: 'Nepermis',
    failed: 'Eșuat',
  },

  outcomeWhy: {
    wouldBePermanent:
      'Coșul de reciclare nu îl poate păstra (este prea mare sau unitatea nu are coș), deci Windows l-ar fi șters definitiv. Vitals l-a lăsat pe loc.',
    missing: 'Nu mai era nimic acolo, deci nu era nimic de mutat.',
    accessDenied:
      'Windows nu i-a permis aplicației Vitals să îl mute. Poate aparține altui cont sau are nevoie de drepturi de administrator.',
    failed: 'Windows nu l-a putut muta (eroarea {{code}}). Este tot acolo unde era.',
  },

  protection: {
    invalid:
      'Aceasta nu este o cale pe care Vitals o poate verifica în siguranță, deci nu a fost atinsă.',
    driveRoot: 'O unitate întreagă nu poate fi trimisă în Coșul de reciclare.',
    systemFolder:
      'Face parte din Windows sau dintr-un program instalat. Ștergerea l-ar strica; dezinstalează programele din ecranul Aplicații.',
    userFolder:
      'Este unul dintre folderele din care este făcut contul tău. Conținutul poate fi verificat, dar folderul rămâne.',
    systemFile: 'Windows îl marchează ca fișier de sistem, deci nu este mutat.',
    noRecycleBin:
      'Această unitate nu are Coș de reciclare, deci orice ștergere de acolo ar fi definitivă. Vitals nu face asta.',
    vitals: 'Aici este instalat chiar Vitals.',
  },

  holders: {
    intro_one: 'Acest program îl ține deschis:',
    intro_few: 'Aceste programe îl țin deschis:',
    intro_other: 'Aceste programe îl țin deschis:',
    pid: 'procesul {{pid}}',
    service: 'serviciul {{name}}',
    unnamed: 'Un program fără nume',
    critical: 'Necesită repornire',
    advice: 'Închide-l (sau salvează și închide fișierul din el), apoi încearcă din nou.',
    none: 'Windows spune că niciun program nu îl mai ține deschis. Poate a fost eliberat chiar acum; încearcă din nou.',
    unknown: 'Este în folosință, dar Windows nu a putut spune de către ce program.',
  },
} as const;

/**
 * Registers the bundle.
 *
 * `deep: true, overwrite: false` — with `deep: false` the merge is shallow and
 * the incoming bundle wins, so `overwrite: false` would protect nothing.
 *
 * Must run after `initI18n`: `addResourceBundle` does not exist before
 * `init()`, and that throw at module scope once left the app on its splash
 * screen forever.
 */
export function registerStorageStrings(): void {
  if (!i18n.isInitialized) {
    throw new Error(
      'registerStorageStrings() was called before initI18n(). i18next only ' +
        'defines addResourceBundle after init, so this must run after the ' +
        'await in bootstrap().',
    );
  }

  i18n.addResourceBundle('en', STORAGE_NS, { ...en, dev: devBundles.en }, true, false);
  i18n.addResourceBundle('ro', STORAGE_NS, { ...ro, dev: devBundles.ro }, true, false);
}

/** Exported for the parity test. */
export const bundles = { en, ro } as const;
