/**
 * Disk storage translations.
 *
 * Own namespace, same reasoning as the other feature bundles. English and
 * Romanian side by side so a key cannot be added to one and forgotten in the
 * other.
 */

import { i18n } from '@vitals/i18n';

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
    fast: 'Fast discovery available',
    fastHint:
      'This drive supports reading its file index directly, which finds folders in seconds. Sizes still come from the full walk, so the scan is not instant.',
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
      'Vitals cannot delete anything yet. Measuring a folder is a read; emptying one is irreversible, and that is not built.',
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
    fast: 'Descoperire rapidă disponibilă',
    fastHint:
      'Această unitate permite citirea directă a indexului de fișiere, ceea ce găsește folderele în câteva secunde. Dimensiunile vin tot din parcurgerea completă, deci scanarea nu este instantanee.',
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
      'Vitals nu poate șterge încă nimic. Măsurarea unui folder este o citire; golirea lui este ireversibilă și nu este implementată.',
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

  i18n.addResourceBundle('en', STORAGE_NS, en, true, false);
  i18n.addResourceBundle('ro', STORAGE_NS, ro, true, false);
}

/** Exported for the parity test. */
export const bundles = { en, ro } as const;
