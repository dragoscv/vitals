/**
 * Guided disk move translations, merged into the storage namespace under
 * `move` when the dialog first renders (it is loaded lazily, after
 * `initI18n()`, so the bundle costs nothing until someone opens it).
 *
 * Written for someone whose drive is full: what moves, what stops, what
 * cannot be interrupted, and that it can be moved back.
 */

import { i18n } from '@vitals/i18n';

import { STORAGE_NS } from '../../strings';

const en = {
  title: 'Move a disk off {{mount}}',
  close: 'Close',
  cancel: 'Cancel',
  back: 'Back',
  next: 'Next',
  loading: 'Looking at the disks on {{mount}} and the drives that could take them',
  loadFailed: 'Could not look at the disks. {{message}}',
  nothing: 'No Linux (WSL) or Docker Desktop disk lives on {{mount}}.',

  disk: {
    heading: 'Which disk',
    distro: 'Linux distribution {{name}}',
    dockerDesktop: 'Docker Desktop data',
    size: 'Size: {{size}}',
    needs: 'A drive needs {{size}} free to take it',
    running: 'Running now',
    unavailable: {
      wslTooOld:
        'This version of WSL cannot move a disk. Run "wsl --update" (version 2.3.11 or later is needed).',
      wslVersionUnknown:
        'Vitals could not read the WSL version, so it does not offer a step that might be refused.',
      wsl1: 'This is a WSL 1 distribution: it has no disk file to move.',
      unsafeName: 'This distribution has a name WSL itself would not accept, so it is not moved.',
    },
  },

  target: {
    heading: 'Where to',
    free: '{{free}} free of {{total}}',
    none: 'No drive on this PC can take this disk right now.',
    noneHint: 'Free some space on another fixed NTFS drive, or add one, and look again.',
    whyNot: 'Not offered: {{reason}}',
    refusal: {
      sameDrive: 'the disk is already on this drive',
      notFixed: 'it can be unplugged or is on the network; a disk there can be lost mid-use',
      notNtfs: 'WSL disks need an NTFS drive',
      tooLittleSpace: 'not enough free space',
      sizeUnknown: 'the size of the disk could not be measured',
      compressedOrEncrypted:
        'the folder is compressed or encrypted, which WSL refuses for its disks',
      occupied: 'a disk file of that name is already in the folder',
    },
  },

  confirm: {
    heading: 'Before it moves',
    from: 'From: {{path}}',
    to: 'To: {{path}}',
    stops: '{{name}} will be stopped while it moves and started again afterwards.',
    noStop: 'Nothing needs to stop: the distribution is not running.',
    noInterrupt:
      'Once copying starts it cannot be interrupted safely. Keep Vitals open; it can take several minutes for a large disk.',
    reversible:
      'Nothing is deleted: WSL copies the disk, switches over, then removes the old file. You can move it back the same way.',
    backup: 'I have a backup of anything important in {{name}}, or I accept the risk',
    run: 'Move now',
  },

  run: {
    working: 'Moving…',
    stage: {
      checking: 'Checking everything again',
      stopping: 'Stopping {{name}}',
      moving: 'Copying the disk to {{mount}}',
      verifying: 'Checking the result',
      restarting: 'Starting {{name}} again',
      other: 'Working',
    },
    copied: '{{done}} of {{total}} copied',
    cannotCancel: 'Copying cannot be interrupted safely. Keep this window open.',
  },

  report: {
    status: {
      moved: '{{name}} now lives on {{mount}}',
      movedWithProblems: '{{name}} moved, but not every check passed',
      notMoved: 'Nothing was moved',
    },
    check: {
      registeredAtTarget: 'WSL points at the new folder',
      fileAtTarget: 'The disk file is in the new folder',
      oldFileGone: 'The old disk file is gone',
      starts: 'The distribution starts',
    },
    pass: 'Passed',
    fail: 'Did not pass',
    notChecked: 'Not checked',
    freed: 'Freed on {{mount}}: {{size}}',
    command: 'Ran: {{command}}',
    restarted: 'Started again:',
    restartFailed: 'Did not start again — do this yourself:',
    message: 'WSL said: {{message}}',
    refused: 'The move was refused before anything stopped: {{message}}',
    failed: 'The move did not finish. {{message}}',
    accessDenied:
      'If the distribution will not start with "E_ACCESSDENIED", give your account Full control of the new folder (right-click > Properties > Security), then try again.',
    done: 'Done',
  },

  docker: {
    heading: 'Docker Desktop moves its own disk',
    why: 'Docker Desktop has no supported command for this, and moving its files behind its back can lose your images and volumes. It moves the data itself from its settings.',
    oldVersion:
      'This Docker Desktop ({{version}}) fails moves to another drive. Update it to 4.92 or later first.',
    steps: {
      one: 'Open Docker Desktop and go to Settings > Resources > Advanced.',
      two: 'Under "Disk image location", choose Browse and pick {{folder}} (create the folder if asked).',
      three: 'Choose "Apply" and wait while Docker Desktop copies the data and restarts.',
      four: 'Come back here and choose "Check again".',
    },
    pickDrive: 'Suggested folder on {{mount}}',
    noTarget: 'No drive on this PC can take Docker Desktop’s disk right now.',
    open: 'Open Docker Desktop',
    check: 'Check again',
    checking: 'Checking…',
    current: 'Now at: {{path}}',
    moved: 'Docker Desktop’s disk is no longer on {{mount}}.',
    notYet: 'Docker Desktop’s disk is still on {{mount}}.',
    unknown: 'Vitals could not find Docker Desktop’s disk.',
    notInstalled: 'Docker Desktop does not seem to be installed.',
  },
};

const ro: typeof en = {
  title: 'Mută un disc virtual de pe {{mount}}',
  close: 'Închide',
  cancel: 'Anulează',
  back: 'Înapoi',
  next: 'Continuă',
  loading: 'Se verifică discurile de pe {{mount}} și unitățile care le-ar putea primi',
  loadFailed: 'Discurile nu au putut fi verificate. {{message}}',
  nothing: 'Pe {{mount}} nu există niciun disc Linux (WSL) sau Docker Desktop.',

  disk: {
    heading: 'Ce disc',
    distro: 'Distribuția Linux {{name}}',
    dockerDesktop: 'Datele Docker Desktop',
    size: 'Mărime: {{size}}',
    needs: 'Unitatea de destinație are nevoie de {{size}} liberi',
    running: 'Rulează acum',
    unavailable: {
      wslTooOld:
        'Această versiune de WSL nu poate muta un disc. Rulează „wsl --update” (e nevoie de 2.3.11 sau mai nouă).',
      wslVersionUnknown:
        'Vitals nu a putut citi versiunea WSL, așa că nu oferă un pas care ar putea fi refuzat.',
      wsl1: 'Este o distribuție WSL 1: nu are un fișier-disc de mutat.',
      unsafeName: 'Distribuția are un nume pe care nici WSL nu l-ar accepta, așa că nu se mută.',
    },
  },

  target: {
    heading: 'Unde',
    free: '{{free}} liberi din {{total}}',
    none: 'Nicio unitate din acest PC nu poate primi acum discul.',
    noneHint: 'Eliberează spațiu pe altă unitate fixă NTFS sau adaugă una, apoi verifică din nou.',
    whyNot: 'Nu este oferită: {{reason}}',
    refusal: {
      sameDrive: 'discul este deja pe această unitate',
      notFixed: 'poate fi deconectată sau este în rețea; un disc de acolo se poate pierde în uz',
      notNtfs: 'discurile WSL au nevoie de o unitate NTFS',
      tooLittleSpace: 'nu are destul spațiu liber',
      sizeUnknown: 'mărimea discului nu a putut fi măsurată',
      compressedOrEncrypted: 'dosarul este comprimat sau criptat, iar WSL refuză astfel de discuri',
      occupied: 'în dosar există deja un fișier-disc cu acest nume',
    },
  },

  confirm: {
    heading: 'Înainte de mutare',
    from: 'De la: {{path}}',
    to: 'La: {{path}}',
    stops: '{{name}} va fi oprită cât durează mutarea și pornită din nou după.',
    noStop: 'Nu trebuie oprit nimic: distribuția nu rulează.',
    noInterrupt:
      'Odată pornită copierea, nu poate fi întreruptă în siguranță. Lasă Vitals deschis; la un disc mare poate dura câteva minute.',
    reversible:
      'Nu se șterge nimic: WSL copiază discul, trece pe noul loc, apoi șterge fișierul vechi. Îl poți muta înapoi la fel.',
    backup: 'Am o copie de siguranță a datelor importante din {{name}} sau accept riscul',
    run: 'Mută acum',
  },

  run: {
    working: 'Se mută…',
    stage: {
      checking: 'Se verifică totul din nou',
      stopping: 'Se oprește {{name}}',
      moving: 'Se copiază discul pe {{mount}}',
      verifying: 'Se verifică rezultatul',
      restarting: 'Se pornește din nou {{name}}',
      other: 'Se lucrează',
    },
    copied: '{{done}} din {{total}} copiați',
    cannotCancel: 'Copierea nu poate fi întreruptă în siguranță. Lasă fereastra deschisă.',
  },

  report: {
    status: {
      moved: '{{name}} se află acum pe {{mount}}',
      movedWithProblems: '{{name}} a fost mutată, dar nu toate verificările au trecut',
      notMoved: 'Nu s-a mutat nimic',
    },
    check: {
      registeredAtTarget: 'WSL indică noul dosar',
      fileAtTarget: 'Fișierul-disc este în noul dosar',
      oldFileGone: 'Fișierul-disc vechi a dispărut',
      starts: 'Distribuția pornește',
    },
    pass: 'Trecută',
    fail: 'Netrecută',
    notChecked: 'Neverificată',
    freed: 'Eliberat pe {{mount}}: {{size}}',
    command: 'S-a rulat: {{command}}',
    restarted: 'Pornite din nou:',
    restartFailed: 'Nu au pornit din nou — fă asta tu:',
    message: 'WSL a spus: {{message}}',
    refused: 'Mutarea a fost refuzată înainte să se oprească ceva: {{message}}',
    failed: 'Mutarea nu s-a terminat. {{message}}',
    accessDenied:
      'Dacă distribuția nu pornește cu „E_ACCESSDENIED”, dă contului tău Control total asupra noului dosar (clic dreapta > Proprietăți > Securitate), apoi încearcă din nou.',
    done: 'Gata',
  },

  docker: {
    heading: 'Docker Desktop își mută singur discul',
    why: 'Docker Desktop nu are o comandă oficială pentru asta, iar mutarea fișierelor fără știrea lui poate pierde imaginile și volumele. Mută datele chiar el, din setări.',
    oldVersion:
      'Această versiune Docker Desktop ({{version}}) eșuează la mutarea pe altă unitate. Actualizeaz-o mai întâi la 4.92 sau mai nouă.',
    steps: {
      one: 'Deschide Docker Desktop și mergi la Settings > Resources > Advanced.',
      two: 'La „Disk image location”, alege Browse și selectează {{folder}} (creează dosarul dacă ți se cere).',
      three: 'Alege „Apply” și așteaptă cât Docker Desktop copiază datele și repornește.',
      four: 'Revino aici și alege „Verifică din nou”.',
    },
    pickDrive: 'Dosar sugerat pe {{mount}}',
    noTarget: 'Nicio unitate din acest PC nu poate primi acum discul Docker Desktop.',
    open: 'Deschide Docker Desktop',
    check: 'Verifică din nou',
    checking: 'Se verifică…',
    current: 'Acum la: {{path}}',
    moved: 'Discul Docker Desktop nu mai este pe {{mount}}.',
    notYet: 'Discul Docker Desktop este încă pe {{mount}}.',
    unknown: 'Vitals nu a găsit discul Docker Desktop.',
    notInstalled: 'Docker Desktop nu pare să fie instalat.',
  },
};

export const moveBundles = { en, ro } as const;

/**
 * Adds the bundles under `storage:move` once. Called while rendering the
 * dialog, which only exists after `initI18n()`; a no-op before it.
 */
export function ensureMoveStrings(): void {
  if (typeof i18n.addResourceBundle !== 'function') return;
  if (i18n.exists(`${STORAGE_NS}:move.title`, { lng: 'en' })) return;
  i18n.addResourceBundle('en', STORAGE_NS, { move: en }, true, false);
  i18n.addResourceBundle('ro', STORAGE_NS, { move: ro }, true, false);
}
