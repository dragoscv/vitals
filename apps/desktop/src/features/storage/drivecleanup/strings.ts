/**
 * Per-drive clean-up translations.
 *
 * Merged into the storage namespace under `drive`, like `dev`: the wizard is
 * opened from the Storage screen and shares its registration.
 *
 * Written for someone whose drive is full and who does not know what a VHDX
 * or a component store is: each item says what it is and what is lost, in
 * one line. Nothing reuses a phrase the Storage tests assert on, because they
 * query the whole screen by text.
 */

const en = {
  open: 'Clean up',
  openLabel: 'Clean up drive {{mount}}',
  title: 'Clean up {{mount}}',
  close: 'Close',
  next: 'Next',
  back: 'Back',
  cancel: 'Cancel',
  stepOf: 'Step {{n}} of 3: {{name}}',
  steps: {
    analyse: 'Analyse',
    choose: 'Choose',
    confirm: 'Confirm',
  },

  analyse: {
    running: 'Measuring what could be cleaned up on {{mount}}',
    starting: 'Starting…',
    progress: '{{done}} of {{total}} checks',
    stop: 'Stop measuring',
    failed: 'The analysis did not finish. {{message}}',
    stage: {
      role: 'Working out what this drive is for',
      windows: 'Checking Windows files',
      temp: 'Checking temporary files',
      caches: 'Measuring package caches',
      artefacts: 'Looking for build output',
      docker: 'Asking Docker',
      wsl: 'Looking inside Linux (WSL)',
      recycleBin: 'Measuring the Recycle Bin',
      blockers: 'Checking what would have to stop',
      other: 'Measuring',
    },
  },

  role: {
    system: 'System drive',
    dev: 'Developer drive',
    data: 'Data drive',
    external: 'External drive',
    network: 'Network drive',
    optical: 'Optical drive',
    why: 'Why Vitals thinks so',
    reason: {
      network: 'It is a shared folder on another computer.',
      optical: 'It is a CD, DVD or Blu-ray drive.',
      holdsWindows: 'Windows is installed here.',
      removable: 'It can be unplugged.',
      data: 'It holds your own files rather than programs.',
      devDrive: 'Windows set it up as a Dev Drive.',
      holdsProjects: 'It holds code projects.',
      holdsPnpmStore: 'pnpm keeps its package store here.',
      holdsWorktrees: 'It holds extra copies of git projects (worktrees).',
      holdsWslDisks: 'Linux (WSL) disks are stored here.',
      holdsDockerDisk: 'Docker Desktop keeps its disk here.',
    },
  },
  free: '{{free}} free of {{total}}',

  group: {
    windows: 'Windows',
    temp: 'Temporary files',
    caches: 'Package caches',
    devArtefacts: 'Build output',
    docker: 'Docker',
    wsl: 'Linux (WSL)',
    compaction: 'Shrink virtual disks',
    sparse: 'Keep virtual disks small',
    recycleBin: 'Recycle Bin',
  },
  groupGain: 'Up to {{size}}',
  groupGainFloor: 'Up to {{size}}, plus {{n}} not measured',
  tickGroup: 'Tick everything safe in {{group}}',
  nothingFound: 'Nothing worth cleaning up was found on this drive.',
  notMeasured: 'Not measured',

  badge: {
    review: 'Check first',
    danger: 'Cannot be undone',
    irreversible: 'Permanent',
    elevation: 'Needs administrator',
    unavailable: 'Not possible here',
  },
  dangerAck: 'I understand {{name}} cannot be undone',
  dangerHint: 'Tick it on its own, then confirm below it. Select-all never ticks this.',

  selection_one: '{{n}} item selected, about {{size}}',
  selection_other: '{{n}} items selected, about {{size}}',
  selectionUnmeasured: '(+ {{n}} not measured)',
  freeAfter: 'Free space afterwards: about {{size}}',

  blockers: {
    restartTitle: 'These will be stopped and started again:',
    stayTitle: 'These will be stopped and stay stopped:',
    kind: {
      wslDistro: 'Linux distribution {{name}}',
      dockerDesktop: 'Docker Desktop',
      container: 'Container {{name}}',
      scheduledTask: 'Scheduled task {{name}}',
      githubRunner: 'GitHub runner {{name}}',
      hyperVVm: 'Virtual machine {{name}}',
      vscodeServer: 'VS Code remote server {{name}}',
    },
    advice: {
      wslDistro: 'Open a terminal and run: wsl -d {{name}}',
      dockerDesktop: 'Start Docker Desktop from the Start menu.',
      container: 'Run: docker start {{name}}',
      scheduledTask: 'Open Task Scheduler and enable {{name}}.',
      githubRunner: 'Start the {{name}} service in Services.',
      hyperVVm: 'Start {{name}} in Hyper-V Manager.',
      vscodeServer: 'Reconnect VS Code; it starts the server again.',
    },
    unknownHyperV:
      'Any running Hyper-V virtual machines (Vitals needs administrator rights to list them)',
    unknownHyperVAdvice: 'Start them again in Hyper-V Manager.',
  },

  usual: {
    title: 'Your usual clean-up',
    body_one: 'You saved {{n}} kind of item for this drive.',
    body_other: 'You saved {{n}} kinds of item for this drive.',
    run: 'Run my usual clean-up',
  },

  confirm: {
    heading: 'You chose',
    irreversible: 'Some of these are deleted for good and cannot be brought back.',
    understood: 'I understand this cannot be undone',
    elevation: 'Windows will ask once for administrator approval.',
    remember: 'Remember my choices for this drive',
    rememberHint: 'Saves the kinds of item, not these exact files, so next time starts from them.',
    run: 'Clean up now',
  },

  run: {
    working: 'Cleaning up…',
    keepOpen: 'Keep this window open until it finishes.',
    progress: '{{done}} of {{total}}',
    current: 'Now: {{name}}',
    stage: {
      cleaning: 'Cleaning up the chosen items',
      stopping: 'Stopping programs that use the disks',
      trimming: 'Freeing unused space inside the disks',
      compacting: 'Shrinking virtual disk {{index}} of {{count}}',
      compactingAll: 'Shrinking virtual disks',
      restarting: 'Starting programs again',
      remeasuring: 'Measuring the drive again',
      other: 'Cleaning up',
    },
  },

  report: {
    title: 'What was cleaned up',
    notRunTitle: 'Nothing was cleaned up',
    status: {
      done: 'Done',
      skipped: 'Skipped',
      failed: 'Failed',
    },
    freed: 'Gave back {{size}}',
    drive: '{{mount}} gained {{size}}',
    driveLost: '{{mount}} has {{size}} less free',
    driveUnknown: 'How much {{mount}} gained could not be measured.',
    restarted: 'Started again:',
    restartFailed: 'Could not be started again. Start them yourself:',
    refused: 'Nothing was changed. {{message}}',
    failed: 'The clean-up did not run. {{message}}',
    done: 'Done',
  },

  item: {
    tempOld: {
      name: 'Old temporary files',
      what: 'Temporary files untouched for over a week. Programs recreate what they need.',
    },
    windowsUpdate: {
      name: 'Windows Update downloads',
      what: 'Installers Windows has already used. Installed updates stay installed.',
    },
    componentStore: {
      name: 'Old Windows components',
      what: 'Replaced copies of Windows files. Updates already installed can no longer be removed.',
    },
    deliveryOptimization: {
      name: 'Update sharing cache',
      what: 'Update pieces kept to share with other PCs. Windows downloads them again if needed.',
    },
    thumbnailCache: {
      name: 'Picture previews',
      what: 'Thumbnails File Explorer remembers. They are redrawn when you next open a folder.',
    },
    minidumps: {
      name: 'Small crash reports',
      what: 'Notes written after past crashes. Only useful for diagnosing those crashes.',
    },
    memoryDump: {
      name: 'Full crash dump',
      what: 'A copy of memory from the last system crash. Only a technician would need it.',
    },
    hibernateOff: {
      name: 'Turn off hibernation',
      what: 'Removes the hibernation file. The PC can no longer hibernate, and fast startup is turned off too.',
    },
    hiberfilOrphan: {
      name: 'Leftover hibernation file',
      what: 'Hibernation is off but its file stayed behind. Nothing uses it.',
    },
    recycleBin: {
      name: 'Recycle Bin',
      what: 'Files you already deleted. Once emptied they cannot be restored.',
    },
    npmCache: {
      name: 'npm cache',
      what: 'Downloaded npm packages. npm fetches them again when a project needs them.',
    },
    pnpmCache: {
      name: 'pnpm metadata cache',
      what: 'Package lists pnpm remembered. Rebuilt on the next install.',
    },
    pnpmStore: {
      name: 'Unused pnpm packages',
      what: 'Packages no project links to any more. pnpm downloads them again if needed.',
    },
    cargoRegistry: {
      name: 'Rust package downloads',
      what: 'Crates Cargo downloaded. Fetched again on the next build.',
    },
    gradleCaches: {
      name: 'Gradle caches',
      what: 'Android and Java build caches. The next build is slower while they refill.',
    },
    nugetCache: {
      name: 'NuGet packages',
      what: '.NET packages kept for builds. Restored on the next build.',
    },
    rustTarget: {
      name: 'Rust build output',
      what: 'Compiled files in target folders. The next build takes longer.',
    },
    nodeModules: {
      name: 'node_modules folders',
      what: "Installed project dependencies. Your package manager's install brings them back.",
    },
    buildDir: {
      name: 'Build output folders',
      what: 'Folders such as dist and build. Recreated the next time you build.',
    },
    dockerBuildCache: {
      name: 'Docker build cache',
      what: 'Layers kept to speed up image builds. The next build is slower.',
    },
    dockerOldImages: {
      name: 'Unused Docker images',
      what: 'Images no container uses. Pulled again if you need them.',
    },
    dockerStoppedContainer: {
      name: 'Stopped Docker container',
      what: 'A container that is not running. Anything saved inside it is lost.',
    },
    dockerAnonVolumes: {
      name: 'Unnamed Docker volumes',
      what: 'Volumes no container uses and nobody named. Their data is lost.',
    },
    dockerNamedVolume: {
      name: 'Named Docker volume',
      what: 'Often a database. Everything in it is deleted for good.',
    },
    wslAptCache: {
      name: 'Linux package downloads',
      what: 'Packages apt has already installed. Downloaded again if needed.',
    },
    wslJournal: {
      name: 'Old Linux system logs',
      what: 'System journal entries older than a few days.',
    },
    wslTmp: {
      name: 'Linux temporary files',
      what: 'Files in /tmp. Programs recreate what they need.',
    },
    wslUserCache: {
      name: 'Linux app caches',
      what: 'Your ~/.cache inside the distribution. Apps rebuild it.',
    },
    wslRustTarget: {
      name: 'Rust build output in Linux',
      what: 'target folders inside the distribution. The next build takes longer.',
    },
    wslBuildDir: {
      name: 'Build output in Linux',
      what: 'node_modules and build folders inside the distribution. Recreated when you build.',
    },
    wslDockerPrune: {
      name: 'Docker leftovers in Linux',
      what: 'Unused images and build cache of the Docker inside the distribution.',
    },
    compactVhd: {
      name: 'Shrink a virtual disk',
      what: 'Gives space freed inside a Linux or Docker disk back to Windows. WSL and Docker stop while it runs.',
    },
    sparseVhd: {
      name: 'Let a virtual disk shrink by itself',
      what: 'From now on the disk file gives space back as it is freed. WSL stops while it switches.',
    },
    other: {
      name: 'Other item',
      what: 'Something this version of Vitals has no description for.',
    },
  },
} as const;

const ro = {
  open: 'Curăță',
  openLabel: 'Curăță unitatea {{mount}}',
  title: 'Curăță {{mount}}',
  close: 'Închide',
  next: 'Înainte',
  back: 'Înapoi',
  cancel: 'Renunță',
  stepOf: 'Pasul {{n}} din 3: {{name}}',
  steps: {
    analyse: 'Analiză',
    choose: 'Alegere',
    confirm: 'Confirmare',
  },

  analyse: {
    running: 'Se măsoară ce se poate curăța pe {{mount}}',
    starting: 'Se pornește…',
    progress: '{{done}} din {{total}} verificări',
    stop: 'Oprește măsurarea',
    failed: 'Analiza nu s-a terminat. {{message}}',
    stage: {
      role: 'Se stabilește la ce folosește unitatea',
      windows: 'Se verifică fișierele Windows',
      temp: 'Se verifică fișierele temporare',
      caches: 'Se măsoară cache-urile de pachete',
      artefacts: 'Se caută rezultate de build',
      docker: 'Se întreabă Docker',
      wsl: 'Se caută în Linux (WSL)',
      recycleBin: 'Se măsoară Coșul de reciclare',
      blockers: 'Se verifică ce ar trebui oprit',
      other: 'Se măsoară',
    },
  },

  role: {
    system: 'Unitatea de sistem',
    dev: 'Unitate de dezvoltare',
    data: 'Unitate de date',
    external: 'Unitate externă',
    network: 'Unitate de rețea',
    optical: 'Unitate optică',
    why: 'De ce crede Vitals asta',
    reason: {
      network: 'Este un folder partajat de pe alt calculator.',
      optical: 'Este o unitate CD, DVD sau Blu-ray.',
      holdsWindows: 'Windows este instalat aici.',
      removable: 'Poate fi deconectată.',
      data: 'Conține fișierele tale, nu programe.',
      devDrive: 'Windows a configurat-o ca Dev Drive.',
      holdsProjects: 'Conține proiecte de cod.',
      holdsPnpmStore: 'pnpm își ține aici depozitul de pachete.',
      holdsWorktrees: 'Conține copii suplimentare ale proiectelor git (worktree-uri).',
      holdsWslDisks: 'Discurile Linux (WSL) sunt păstrate aici.',
      holdsDockerDisk: 'Docker Desktop își ține discul aici.',
    },
  },
  free: '{{free}} liber din {{total}}',

  group: {
    windows: 'Windows',
    temp: 'Fișiere temporare',
    caches: 'Cache-uri de pachete',
    devArtefacts: 'Rezultate de build',
    docker: 'Docker',
    wsl: 'Linux (WSL)',
    compaction: 'Micșorează discurile virtuale',
    sparse: 'Păstrează discurile virtuale mici',
    recycleBin: 'Coșul de reciclare',
  },
  groupGain: 'Până la {{size}}',
  groupGainFloor: 'Până la {{size}}, plus {{n}} nemăsurate',
  tickGroup: 'Bifează tot ce este sigur în {{group}}',
  nothingFound: 'Nu s-a găsit nimic care merită curățat pe această unitate.',
  notMeasured: 'Nemăsurat',

  badge: {
    review: 'Verifică întâi',
    danger: 'Nu se poate anula',
    irreversible: 'Definitiv',
    elevation: 'Necesită administrator',
    unavailable: 'Nu se poate de aici',
  },
  dangerAck: 'Înțeleg că {{name}} nu poate fi recuperat',
  dangerHint:
    'Bifează-l separat, apoi confirmă dedesubt. Selectarea tuturor nu îl bifează niciodată.',

  selection_one: '{{n}} element selectat, aproximativ {{size}}',
  selection_few: '{{n}} elemente selectate, aproximativ {{size}}',
  selection_other: '{{n}} de elemente selectate, aproximativ {{size}}',
  selectionUnmeasured: '(+ {{n}} nemăsurate)',
  freeAfter: 'Spațiu liber după: aproximativ {{size}}',

  blockers: {
    restartTitle: 'Acestea vor fi oprite și pornite din nou:',
    stayTitle: 'Acestea vor fi oprite și rămân oprite:',
    kind: {
      wslDistro: 'Distribuția Linux {{name}}',
      dockerDesktop: 'Docker Desktop',
      container: 'Containerul {{name}}',
      scheduledTask: 'Sarcina programată {{name}}',
      githubRunner: 'Runner-ul GitHub {{name}}',
      hyperVVm: 'Mașina virtuală {{name}}',
      vscodeServer: 'Serverul la distanță VS Code {{name}}',
    },
    advice: {
      wslDistro: 'Deschide un terminal și rulează: wsl -d {{name}}',
      dockerDesktop: 'Pornește Docker Desktop din meniul Start.',
      container: 'Rulează: docker start {{name}}',
      scheduledTask: 'Deschide Task Scheduler și activează {{name}}.',
      githubRunner: 'Pornește serviciul {{name}} din Servicii.',
      hyperVVm: 'Pornește {{name}} din Hyper-V Manager.',
      vscodeServer: 'Reconectează VS Code; acesta pornește din nou serverul.',
    },
    unknownHyperV:
      'Orice mașină virtuală Hyper-V pornită (Vitals are nevoie de drepturi de administrator ca să le vadă)',
    unknownHyperVAdvice: 'Pornește-le din nou din Hyper-V Manager.',
  },

  usual: {
    title: 'Curățenia ta obișnuită',
    body_one: 'Ai salvat {{n}} tip de element pentru această unitate.',
    body_few: 'Ai salvat {{n}} tipuri de elemente pentru această unitate.',
    body_other: 'Ai salvat {{n}} de tipuri de elemente pentru această unitate.',
    run: 'Rulează curățenia obișnuită',
  },

  confirm: {
    heading: 'Ai ales',
    irreversible: 'Unele dintre acestea sunt șterse definitiv și nu mai pot fi recuperate.',
    understood: 'Înțeleg că acest lucru nu poate fi anulat',
    elevation: 'Windows va cere o singură dată aprobarea de administrator.',
    remember: 'Ține minte alegerile mele pentru această unitate',
    rememberHint:
      'Salvează tipurile de elemente, nu aceste fișiere anume, ca data viitoare să pornești de la ele.',
    run: 'Curăță acum',
  },

  run: {
    working: 'Se curăță…',
    keepOpen: 'Lasă fereastra deschisă până se termină.',
    progress: '{{done}} din {{total}}',
    current: 'Acum: {{name}}',
    stage: {
      cleaning: 'Se curăță elementele alese',
      stopping: 'Se opresc programele care folosesc discurile',
      trimming: 'Se eliberează spațiul nefolosit din discuri',
      compacting: 'Se micșorează discul virtual {{index}} din {{count}}',
      compactingAll: 'Se micșorează discurile virtuale',
      restarting: 'Programele pornesc din nou',
      remeasuring: 'Se măsoară din nou unitatea',
      other: 'Se curăță',
    },
  },

  report: {
    title: 'Ce s-a curățat',
    notRunTitle: 'Nu s-a curățat nimic',
    status: {
      done: 'Făcut',
      skipped: 'Sărit',
      failed: 'Eșuat',
    },
    freed: 'S-au recuperat {{size}}',
    drive: '{{mount}} a câștigat {{size}}',
    driveLost: '{{mount}} are cu {{size}} mai puțin liber',
    driveUnknown: 'Cât a câștigat {{mount}} nu a putut fi măsurat.',
    restarted: 'Pornite din nou:',
    restartFailed: 'Nu au putut fi pornite din nou. Pornește-le tu:',
    refused: 'Nu s-a schimbat nimic. {{message}}',
    failed: 'Curățenia nu a rulat. {{message}}',
    done: 'Gata',
  },

  item: {
    tempOld: {
      name: 'Fișiere temporare vechi',
      what: 'Fișiere temporare neatinse de peste o săptămână. Programele recreează ce le trebuie.',
    },
    windowsUpdate: {
      name: 'Descărcări Windows Update',
      what: 'Programe de instalare pe care Windows le-a folosit deja. Actualizările instalate rămân.',
    },
    componentStore: {
      name: 'Componente Windows vechi',
      what: 'Copii înlocuite ale fișierelor Windows. Actualizările instalate nu mai pot fi dezinstalate.',
    },
    deliveryOptimization: {
      name: 'Cache-ul de partajare a actualizărilor',
      what: 'Bucăți de actualizări păstrate pentru alte PC-uri. Windows le descarcă din nou la nevoie.',
    },
    thumbnailCache: {
      name: 'Previzualizări de imagini',
      what: 'Miniaturile reținute de File Explorer. Sunt redesenate când deschizi din nou un folder.',
    },
    minidumps: {
      name: 'Rapoarte mici de blocare',
      what: 'Note scrise după blocări trecute. Utile doar pentru a diagnostica acele blocări.',
    },
    memoryDump: {
      name: 'Copie completă de memorie la blocare',
      what: 'Memoria de la ultima blocare a sistemului. Doar unui tehnician i-ar folosi.',
    },
    hibernateOff: {
      name: 'Oprește hibernarea',
      what: 'Șterge fișierul de hibernare. PC-ul nu mai poate hiberna, iar pornirea rapidă se oprește și ea.',
    },
    hiberfilOrphan: {
      name: 'Fișier de hibernare rămas',
      what: 'Hibernarea este oprită, dar fișierul ei a rămas. Nu îl folosește nimic.',
    },
    recycleBin: {
      name: 'Coșul de reciclare',
      what: 'Fișiere pe care le-ai șters deja. După golire nu mai pot fi recuperate.',
    },
    npmCache: {
      name: 'Cache-ul npm',
      what: 'Pachete npm descărcate. npm le aduce din nou când un proiect are nevoie de ele.',
    },
    pnpmCache: {
      name: 'Cache-ul de metadate pnpm',
      what: 'Liste de pachete reținute de pnpm. Refăcute la următoarea instalare.',
    },
    pnpmStore: {
      name: 'Pachete pnpm nefolosite',
      what: 'Pachete la care nu mai trimite niciun proiect. pnpm le descarcă din nou la nevoie.',
    },
    cargoRegistry: {
      name: 'Descărcări de pachete Rust',
      what: 'Crate-uri descărcate de Cargo. Aduse din nou la următorul build.',
    },
    gradleCaches: {
      name: 'Cache-uri Gradle',
      what: 'Cache-uri de build Android și Java. Următorul build e mai lent până se refac.',
    },
    nugetCache: {
      name: 'Pachete NuGet',
      what: 'Pachete .NET păstrate pentru build. Refăcute la următorul build.',
    },
    rustTarget: {
      name: 'Rezultate de build Rust',
      what: 'Fișiere compilate din folderele target. Următorul build durează mai mult.',
    },
    nodeModules: {
      name: 'Foldere node_modules',
      what: 'Dependențele instalate ale proiectelor. Comanda install a managerului de pachete le readuce.',
    },
    buildDir: {
      name: 'Foldere cu rezultate de build',
      what: 'Foldere precum dist și build. Recreate la următorul build.',
    },
    dockerBuildCache: {
      name: 'Cache-ul de build Docker',
      what: 'Straturi păstrate pentru build-uri mai rapide. Următorul build e mai lent.',
    },
    dockerOldImages: {
      name: 'Imagini Docker nefolosite',
      what: 'Imagini pe care nu le folosește niciun container. Descărcate din nou la nevoie.',
    },
    dockerStoppedContainer: {
      name: 'Container Docker oprit',
      what: 'Un container care nu rulează. Tot ce s-a salvat în el se pierde.',
    },
    dockerAnonVolumes: {
      name: 'Volume Docker fără nume',
      what: 'Volume nefolosite de niciun container și fără nume. Datele lor se pierd.',
    },
    dockerNamedVolume: {
      name: 'Volum Docker cu nume',
      what: 'Adesea o bază de date. Tot ce conține este șters definitiv.',
    },
    wslAptCache: {
      name: 'Descărcări de pachete Linux',
      what: 'Pachete pe care apt le-a instalat deja. Descărcate din nou la nevoie.',
    },
    wslJournal: {
      name: 'Jurnale vechi de sistem Linux',
      what: 'Intrări din jurnalul de sistem mai vechi de câteva zile.',
    },
    wslTmp: {
      name: 'Fișiere temporare Linux',
      what: 'Fișierele din /tmp. Programele recreează ce le trebuie.',
    },
    wslUserCache: {
      name: 'Cache-uri de aplicații Linux',
      what: 'Folderul ~/.cache din distribuție. Aplicațiile îl refac.',
    },
    wslRustTarget: {
      name: 'Rezultate de build Rust în Linux',
      what: 'Folderele target din distribuție. Următorul build durează mai mult.',
    },
    wslBuildDir: {
      name: 'Rezultate de build în Linux',
      what: 'Foldere node_modules și build din distribuție. Recreate la build.',
    },
    wslDockerPrune: {
      name: 'Resturi Docker în Linux',
      what: 'Imagini nefolosite și cache de build ale Docker-ului din distribuție.',
    },
    compactVhd: {
      name: 'Micșorează un disc virtual',
      what: 'Dă înapoi Windows spațiul eliberat într-un disc Linux sau Docker. WSL și Docker se opresc cât durează.',
    },
    sparseVhd: {
      name: 'Lasă un disc virtual să se micșoreze singur',
      what: 'De acum fișierul discului dă înapoi spațiul pe măsură ce este eliberat. WSL se oprește cât se face schimbarea.',
    },
    other: {
      name: 'Alt element',
      what: 'Ceva pentru care această versiune Vitals nu are o descriere.',
    },
  },
} as const;

/** Merged into the storage bundles under `drive`. */
export const driveBundles = { en, ro } as const;
