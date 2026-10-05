/**
 * Developer cleanup translations.
 *
 * Merged into the storage namespace under `dev` rather than a namespace of
 * its own: the card lives on the Storage screen and shares its
 * registration, so there is no second `register…()` a test or the bootstrap
 * can forget to call.
 *
 * Nothing here says "freed" before anything has run, or reuses a phrase the
 * rest of the screen asserts on: the Storage tests query the whole screen by
 * text, and this card is on it.
 */

const en = {
  title: 'Developer cleanup',
  intro:
    'Build output, dependency folders, caches and old worktrees that your tools can recreate. You choose what goes.',
  noHost: 'Available in the desktop app.',

  roots: {
    heading: 'Folders to look in',
    add: 'Add folder…',
    remove: 'Stop looking in {{path}}',
    empty: 'No folders chosen. Add one to scan.',
    loading: 'Finding your usual project folders…',
    failed: 'Your usual project folders could not be listed. {{message}}',
  },

  scan: {
    start: 'Scan projects',
    rescan: 'Scan projects again',
    stop: 'Stop scanning projects',
    running: 'Looking for developer clutter',
    found_one: '{{n}} item found so far',
    found_other: '{{n}} items found so far',
    now: 'Reading {{path}}',
    failed: 'The project scan did not finish. {{message}}',
    stale: 'Showing the last finished project scan. {{message}}',
    idle: 'Choose where to look, then scan. Nothing is removed until you confirm.',
    elapsed: 'Scanned in {{seconds}} s',
    nothing: 'Nothing to clean up was found in these folders.',
  },

  phase: {
    projects: 'Finding projects',
    sizing: 'Measuring folders',
    worktrees: 'Checking git worktrees',
    caches: 'Measuring package caches',
    docker: 'Asking Docker',
    vdisks: 'Checking virtual disks',
  },

  section: {
    projects: 'Projects',
    worktrees: 'Git worktrees',
    caches: 'Package caches',
    docker: 'Docker',
    vdisks: 'Virtual disks',
  },

  total: '{{size}} in total',
  totalFloor: '{{size}} in total, plus {{n}} not measured',
  selectAll: 'Select all in {{section}}',
  selectNone: 'Select none in {{section}}',
  menu: {
    select: 'Tick for clean-up',
    deselect: 'Untick',
    expand: 'Show its folders',
    collapse: 'Hide its folders',
    copyCommand: 'Copy command',
  },
  notMeasured: 'Not measured',
  restore: 'Bring it back with',
  runs: 'Runs',

  project: {
    idle_one: 'Idle {{count}} day',
    idle_other: 'Idle {{count}} days',
    active: 'Active',
    idleUnknown: 'Idle unknown',
    git: 'Git',
    notGit: 'Not a git repository',
    show: 'Show the folders in {{name}}',
    hide: 'Hide the folders in {{name}}',
    select: 'Select every folder in {{name}}',
    onlyStale: 'Only idle 30+ days',
    none: 'No projects with build output were found.',
    filteredOut: 'No project has been idle for 30 days or more.',
  },

  artefact: {
    nodeModules: 'node_modules',
    cargoTarget: 'Rust target',
    next: '.next',
    turbo: '.turbo',
    dist: 'dist',
    build: 'build',
    gradle: 'Gradle build',
    pycache: '__pycache__',
    venv: 'Python virtual environment',
    pytestCache: '.pytest_cache',
    parcelCache: '.parcel-cache',
    svelteKit: '.svelte-kit',
    nuxt: '.nuxt',
    coverage: 'coverage',
    expo: '.expo',
  },
  artefactSelect: 'Select {{label}} at {{path}}',
  sharedPnpm:
    'Most of this is shared with the pnpm store, so removing it gives back little until the pnpm store is also pruned.',

  worktree: {
    none: 'No linked worktrees were found.',
    branch: 'branch {{branch}}',
    detached: 'no branch',
    select: 'Select the worktree at {{path}}',
    removable: 'Clean, pushed and idle',
    prunable: 'Folder already gone - tidy git’s record',
    dirty_one: 'Kept: {{count}} uncommitted change',
    dirty_other: 'Kept: {{count}} uncommitted changes',
    dirtyUnknown: 'Kept: has uncommitted changes',
    unpushed: 'Kept: has commits that were never pushed',
    active_one: 'Kept: active {{count}} hour ago',
    active_other: 'Kept: active {{count}} hours ago',
    activeUnknown: 'Kept: in use recently',
    locked: 'Kept: git has it locked',
  },

  worktreeState: {
    removable: 'Worktree',
    prunable: 'Worktree record',
    dirty: 'Worktree with changes',
    unpushed: 'Worktree with unpushed commits',
    active: 'Active worktree',
    locked: 'Locked worktree',
  },

  cache: {
    pnpm: 'pnpm store',
    npm: 'npm cache',
    yarn: 'Yarn cache',
    cargo: 'Cargo registry',
    gradle: 'Gradle caches',
    nuget: 'NuGet packages',
    pip: 'pip cache',
    uv: 'uv cache',
    go: 'Go module cache',
    playwright: 'Playwright browsers',
    electron: 'Electron downloads',
  },
  cacheNone: 'No package caches were found.',
  cacheMissing: 'Tool not installed',
  cacheSelect: 'Select {{label}}',

  docker: {
    danglingImages: 'Dangling images',
    unusedImages: 'Unused images',
    buildCache: 'Build cache',
    stoppedContainers: 'Stopped containers',
    volumes: 'Volumes',
  },
  dockerState: {
    notInstalled: 'Docker is not installed.',
    notRunning:
      'Docker is installed but not running. Start Docker Desktop and scan again to see what it holds.',
  },
  dockerNone: 'Docker has nothing to reclaim.',
  dockerReclaimable: 'Docker says {{size}} can be reclaimed',
  dockerCount_one: '{{n}} item',
  dockerCount_other: '{{n}} items',
  dockerVolumes: 'Never removed by Vitals - they hold databases.',
  dockerSelect: 'Select {{label}}',
  dockerDisk:
    'Docker gives space back inside its own virtual disk. To return it to Windows, also compact the disk under Virtual disks.',

  vdisk: {
    docker: 'Docker Desktop disk',
    wsl: 'WSL disk',
  },
  vdiskNone: 'No WSL or Docker virtual disks were found.',
  vdiskSize: 'File size {{size}}',
  vdiskDistro: 'Distribution {{distro}}',
  vdiskSelect: 'Compact {{label}} at {{path}}',
  vdiskNote:
    'Compacting stops WSL and Docker Desktop while it runs, pauses scheduled tasks that would start WSL again, and asks for administrator approval once. Everything is started again afterwards.',

  summary_one: '{{n}} item selected - {{size}}',
  summary_other: '{{n}} items selected - {{size}}',
  summaryUnmeasured: '(+ {{n}} not measured)',
  review: 'Clean up…',

  confirm: {
    title: 'Clean up these items?',
    permanent:
      'Folders are deleted permanently. They do not go to the Recycle Bin. Each can be recreated with the command shown.',
    permanentBadge: 'Permanent',
    stopsWsl:
      'WSL, Docker Desktop and any scheduled task that starts WSL will stop while their disks are compacted, and start again afterwards.',
    elevation: 'Windows will ask for administrator approval once.',
    understood: 'I understand this cannot be undone',
    cancel: 'Cancel',
    run: 'Delete permanently',
    running: 'Cleaning up…',
    progress: '{{index}} of {{total}}',
    close: 'Close',
    done: 'Done',
    reportTitle: 'What was cleaned up',
    notRunTitle: 'Nothing was cleaned up',
  },

  outcome: {
    done: 'Done',
    partial: 'Partly done',
    failed: 'Failed',
    refused: 'Not done',
    unavailable: 'Unavailable',
  },

  report: {
    freed: 'Gave back {{size}}',
    freedUnknown: 'Space given back: not measured',
    toolFreed: 'Docker says {{size}}',
    drive: 'Drive {{drive}} gained {{size}}',
    driveLost: 'Drive {{drive}} has {{size}} less free',
    refused: 'Nothing was changed for this item.',
    holders: 'Still held open by:',
    holder: '{{name}} (process {{pid}})',
    refusedAll: 'Nothing was changed. {{message}}',
    failed: 'The clean-up did not run. {{message}}',
  },
} as const;

const ro = {
  title: 'Curățenie pentru dezvoltatori',
  intro:
    'Rezultate de build, foldere de dependențe, cache-uri și worktree-uri vechi pe care uneltele tale le pot recrea. Tu alegi ce pleacă.',
  noHost: 'Disponibil în aplicația desktop.',

  roots: {
    heading: 'Foldere în care se caută',
    add: 'Adaugă folder…',
    remove: 'Nu mai căuta în {{path}}',
    empty: 'Niciun folder ales. Adaugă unul ca să scanezi.',
    loading: 'Se caută folderele obișnuite de proiecte…',
    failed: 'Folderele obișnuite de proiecte nu au putut fi listate. {{message}}',
  },

  scan: {
    start: 'Scanează proiectele',
    rescan: 'Scanează din nou proiectele',
    stop: 'Oprește scanarea proiectelor',
    running: 'Se caută resturile de dezvoltare',
    found_one: '{{n}} element găsit până acum',
    found_few: '{{n}} elemente găsite până acum',
    found_other: '{{n}} de elemente găsite până acum',
    now: 'Se citește {{path}}',
    failed: 'Scanarea proiectelor nu s-a terminat. {{message}}',
    stale: 'Se afișează ultima scanare terminată a proiectelor. {{message}}',
    idle: 'Alege unde să caute, apoi scanează. Nimic nu este șters până nu confirmi.',
    elapsed: 'Scanat în {{seconds}} s',
    nothing: 'Nu s-a găsit nimic de curățat în aceste foldere.',
  },

  phase: {
    projects: 'Se caută proiectele',
    sizing: 'Se măsoară folderele',
    worktrees: 'Se verifică worktree-urile git',
    caches: 'Se măsoară cache-urile de pachete',
    docker: 'Se întreabă Docker',
    vdisks: 'Se verifică discurile virtuale',
  },

  section: {
    projects: 'Proiecte',
    worktrees: 'Worktree-uri git',
    caches: 'Cache-uri de pachete',
    docker: 'Docker',
    vdisks: 'Discuri virtuale',
  },

  total: '{{size}} în total',
  totalFloor: '{{size}} în total, plus {{n}} nemăsurate',
  selectAll: 'Selectează tot din {{section}}',
  selectNone: 'Deselectează tot din {{section}}',
  menu: {
    select: 'Bifează pentru curățenie',
    deselect: 'Debifează',
    expand: 'Arată folderele',
    collapse: 'Ascunde folderele',
    copyCommand: 'Copiază comanda',
  },
  notMeasured: 'Nemăsurat',
  restore: 'Se recreează cu',
  runs: 'Rulează',

  project: {
    idle_one: 'Neatins de {{count}} zi',
    idle_few: 'Neatins de {{count}} zile',
    idle_other: 'Neatins de {{count}} de zile',
    active: 'Activ',
    idleUnknown: 'Vechime necunoscută',
    git: 'Git',
    notGit: 'Nu este un depozit git',
    show: 'Arată folderele din {{name}}',
    hide: 'Ascunde folderele din {{name}}',
    select: 'Selectează toate folderele din {{name}}',
    onlyStale: 'Doar neatinse de 30+ zile',
    none: 'Nu s-au găsit proiecte cu rezultate de build.',
    filteredOut: 'Niciun proiect nu a stat neatins 30 de zile sau mai mult.',
  },

  artefact: {
    nodeModules: 'node_modules',
    cargoTarget: 'target Rust',
    next: '.next',
    turbo: '.turbo',
    dist: 'dist',
    build: 'build',
    gradle: 'build Gradle',
    pycache: '__pycache__',
    venv: 'mediu virtual Python',
    pytestCache: '.pytest_cache',
    parcelCache: '.parcel-cache',
    svelteKit: '.svelte-kit',
    nuxt: '.nuxt',
    coverage: 'coverage',
    expo: '.expo',
  },
  artefactSelect: 'Selectează {{label}} din {{path}}',
  sharedPnpm:
    'Cea mai mare parte este comună cu depozitul pnpm, așa că ștergerea recuperează puțin până nu este curățat și depozitul pnpm.',

  worktree: {
    none: 'Nu s-au găsit worktree-uri legate.',
    branch: 'ramura {{branch}}',
    detached: 'fără ramură',
    select: 'Selectează worktree-ul din {{path}}',
    removable: 'Curat, publicat și neatins',
    prunable: 'Folderul nu mai există - curăță evidența din git',
    dirty_one: 'Păstrat: {{count}} modificare necomisă',
    dirty_few: 'Păstrat: {{count}} modificări necomise',
    dirty_other: 'Păstrat: {{count}} de modificări necomise',
    dirtyUnknown: 'Păstrat: are modificări necomise',
    unpushed: 'Păstrat: are commit-uri nepublicate',
    active_one: 'Păstrat: activ acum {{count}} oră',
    active_few: 'Păstrat: activ acum {{count}} ore',
    active_other: 'Păstrat: activ acum {{count}} de ore',
    activeUnknown: 'Păstrat: folosit recent',
    locked: 'Păstrat: git îl ține blocat',
  },

  worktreeState: {
    removable: 'Worktree',
    prunable: 'Evidență de worktree',
    dirty: 'Worktree cu modificări',
    unpushed: 'Worktree cu commit-uri nepublicate',
    active: 'Worktree activ',
    locked: 'Worktree blocat',
  },

  cache: {
    pnpm: 'depozitul pnpm',
    npm: 'cache-ul npm',
    yarn: 'cache-ul Yarn',
    cargo: 'registrul Cargo',
    gradle: 'cache-urile Gradle',
    nuget: 'pachetele NuGet',
    pip: 'cache-ul pip',
    uv: 'cache-ul uv',
    go: 'cache-ul de module Go',
    playwright: 'browserele Playwright',
    electron: 'descărcările Electron',
  },
  cacheNone: 'Nu s-au găsit cache-uri de pachete.',
  cacheMissing: 'Unealta nu este instalată',
  cacheSelect: 'Selectează {{label}}',

  docker: {
    danglingImages: 'Imagini orfane',
    unusedImages: 'Imagini nefolosite',
    buildCache: 'Cache de build',
    stoppedContainers: 'Containere oprite',
    volumes: 'Volume',
  },
  dockerState: {
    notInstalled: 'Docker nu este instalat.',
    notRunning:
      'Docker este instalat, dar nu rulează. Pornește Docker Desktop și scanează din nou ca să vezi ce conține.',
  },
  dockerNone: 'Docker nu are nimic de recuperat.',
  dockerReclaimable: 'Docker spune că se pot recupera {{size}}',
  dockerCount_one: '{{n}} element',
  dockerCount_few: '{{n}} elemente',
  dockerCount_other: '{{n}} de elemente',
  dockerVolumes: 'Nu sunt șterse niciodată de Vitals - conțin baze de date.',
  dockerSelect: 'Selectează {{label}}',
  dockerDisk:
    'Docker eliberează spațiu în propriul disc virtual. Ca să-l dai înapoi Windows, compactează și discul, la Discuri virtuale.',

  vdisk: {
    docker: 'Discul Docker Desktop',
    wsl: 'Disc WSL',
  },
  vdiskNone: 'Nu s-au găsit discuri virtuale WSL sau Docker.',
  vdiskSize: 'Dimensiunea fișierului {{size}}',
  vdiskDistro: 'Distribuția {{distro}}',
  vdiskSelect: 'Compactează {{label}} din {{path}}',
  vdiskNote:
    'Compactarea oprește WSL și Docker Desktop cât durează, suspendă sarcinile programate care ar reporni WSL și cere o singură dată aprobarea de administrator. La final, totul pornește din nou.',

  summary_one: '{{n}} element selectat - {{size}}',
  summary_few: '{{n}} elemente selectate - {{size}}',
  summary_other: '{{n}} de elemente selectate - {{size}}',
  summaryUnmeasured: '(+ {{n}} nemăsurate)',
  review: 'Curăță…',

  confirm: {
    title: 'Curăți aceste elemente?',
    permanent:
      'Folderele sunt șterse definitiv. Nu ajung în Coșul de reciclare. Fiecare poate fi recreat cu comanda afișată.',
    permanentBadge: 'Definitiv',
    stopsWsl:
      'WSL, Docker Desktop și orice sarcină programată care pornește WSL se vor opri cât timp le sunt compactate discurile, apoi vor porni din nou.',
    elevation: 'Windows va cere o singură dată aprobarea de administrator.',
    understood: 'Înțeleg că acest lucru nu poate fi anulat',
    cancel: 'Renunță',
    run: 'Șterge definitiv',
    running: 'Se curăță…',
    progress: '{{index}} din {{total}}',
    close: 'Închide',
    done: 'Gata',
    reportTitle: 'Ce s-a curățat',
    notRunTitle: 'Nu s-a curățat nimic',
  },

  outcome: {
    done: 'Făcut',
    partial: 'Făcut parțial',
    failed: 'Eșuat',
    refused: 'Nefăcut',
    unavailable: 'Indisponibil',
  },

  report: {
    freed: 'S-au recuperat {{size}}',
    freedUnknown: 'Spațiu recuperat: nemăsurat',
    toolFreed: 'Docker spune {{size}}',
    drive: 'Unitatea {{drive}} a câștigat {{size}}',
    driveLost: 'Unitatea {{drive}} are cu {{size}} mai puțin liber',
    refused: 'Nu s-a schimbat nimic pentru acest element.',
    holders: 'Încă ținut deschis de:',
    holder: '{{name}} (procesul {{pid}})',
    refusedAll: 'Nu s-a schimbat nimic. {{message}}',
    failed: 'Curățenia nu a rulat. {{message}}',
  },
} as const;

/** Merged into the storage bundles under `dev`. */
export const devBundles = { en, ro } as const;
