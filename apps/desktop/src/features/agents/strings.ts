/**
 * AI page translations. English and Romanian side by side so a key cannot be
 * added to one and forgotten in the other.
 */

import { i18n } from '@vitals/i18n';

export const AGENTS_NS = 'agents';

const en = {
  title: 'AI agents',
  subtitle: 'Which AI agents are working on this machine, what they wait for, and what it costs.',
  live: 'Live',
  updated: 'Updated {{ago}} ago',

  tab: {
    overview: 'Overview',
    sessions: 'Sessions',
    queues: 'Queues',
    worktrees: 'Worktrees',
    jobs: 'Jobs & releases',
    journal: 'Journal',
    cloud: 'Cloud',
    insights: 'Insights',
  },

  kpi: {
    working: 'Working now',
    waiting: 'Waiting',
    idle: 'Waiting for you',
    building: 'Building',
    queued: 'In queue',
    stale: 'Stale or dead holders',
    jobs: 'Background jobs',
    suspect: 'Need attention',
    leases: 'Worktrees in use',
    cap: 'Build slots',
  },

  client: {
    vscodeCopilot: 'VS Code Copilot Chat',
    copilotCli: 'Copilot CLI',
    codai: 'Codai',
    codex: 'Codex',
    claudeCode: 'Claude Code',
    other: 'Other',
  },

  state: {
    working: 'Working',
    waiting: 'Waiting',
    idle: 'Waiting for you',
    ended: 'Ended',
  },

  liveness: {
    alive: 'Alive',
    stale: 'Stale',
    dead: 'Dead',
    unknown: 'Unknown',
  },

  session: {
    started: 'Started',
    lastActivity: 'Last activity',
    active: 'Active time',
    activeShare: '{{pct}} of the session spent working',
    turns: 'Turns',
    prompts: 'Your messages',
    tools: 'Tool calls',
    failures: 'Failed tools',
    tokens: 'Tokens in / out',
    model: 'Model',
    branch: 'Branch',
    nowRunning: 'Running {{tool}}',
    topTools: 'Most used tools',
    agentqWork: 'Through agentq',
    holds: 'Holds {{resource}}',
    waitsFor: 'Waits for {{resource}}',
    lease: 'Worktree {{slot}} on {{branch}}',
    noWork: 'No agentq queue or worktree right now.',
    untitled: 'Untitled session',
  },

  queue: {
    capacity: '{{held}} of {{capacity}} in use',
    waiting_one: '{{count}} waiting',
    waiting_other: '{{count}} waiting',
    held: 'Running',
    waited: 'Waiting {{ago}}',
    heldFor: 'Running for {{ago}}',
    lane: 'Lane {{lane}}',
    yielded: 'Idle (yielded its slot)',
    wedged: 'Build log went quiet',
    empty: 'No queues are busy.',
    frozen: 'Frozen',
    blocked: 'Blocked',
  },

  machine: {
    title: 'Build slots on this machine',
    adaptive: 'Adapts between {{min}} and {{max}} to CPU and memory',
    why: 'Last decision: {{why}}',
    cpu: 'CPU (average)',
    ram: 'Free memory',
    commit: 'Free commit',
  },

  lease: {
    owner: 'Owner',
    alive: 'Owner running',
    gone: 'Owner gone',
    stale: 'Stale — agentq will reclaim it',
    idle: 'Idle {{ago}}',
    empty: 'No worktree is leased.',
  },

  job: {
    expected: 'Expected {{min}} min',
    elapsed: 'Running {{ago}}',
    silent: 'Log silent {{ago}}',
    exit: 'Exit {{code}}',
    state: {
      running: 'Running',
      done: 'Done',
      failed: 'Failed',
      killed: 'Killed',
      suspect: 'Suspect',
      dead: 'Dead',
      unknown: 'Unknown',
    },
    empty: 'No background jobs in the last day.',
    releases: 'Releases',
    releasesEmpty: 'No release in the last two days.',
    phase: 'Phase {{phase}}',
  },

  journal: {
    filter: 'Filter events',
    all: 'All',
    problems: 'Problems',
    empty: 'No events match.',
  },

  cloud: {
    title: 'Cloud',
    down: '{{ws}} is unavailable until {{until}} — runs go local.',
    downReason: 'Reason: {{reason}}',
    healthy: 'No cloud workspace is marked down.',
    runs: 'Cloud runs in the last {{hours}} h',
    signIn: 'Sign in to codai to see agents running in the cloud',
    signInBody:
      'Cloud sessions, environments and usage come from your codai account. Nothing is sent until you sign in.',
    signInButton: 'Sign in with codai',
    waiting: 'Waiting for approval in the browser…',
    code: 'Confirm this code in the browser:',
    account: 'codai account',
    hosts: 'Connected desktops',
    noHosts: 'No desktop is connected to codai right now.',
    signOut: 'Sign out',
    sessions_one: '{{count}} cloud session in the last day, listed under Sessions.',
    sessions_other: '{{count}} cloud sessions in the last day, listed under Sessions.',
  },

  insights: {
    window: 'Last {{hours}} hours',
    runs: 'Runs',
    failures: 'Failures',
    commits: 'Commits',
    waitP50: 'Typical wait',
    waitP90: 'Slow wait (p90)',
    holdP50: 'Typical build',
    holdP90: 'Slow build (p90)',
    waitTotal: 'Time agents spent waiting',
    perHour: 'Runs per hour',
    byRepo: 'By repository',
    repoRow: '{{runs}} runs · {{fail}} failed · {{hold}} building · {{wait}} waiting',
  },

  action: {
    freeze: 'Freeze',
    clear: 'Clear',
    break: 'Remove ticket',
    unlease: 'Release worktree',
    killJob: 'Stop job',
    note: 'Leave a note',
    reap: 'Close idle terminals',
    reapPreview: 'Preview',
    reason: 'Reason (other agents read it)',
    run: 'Run',
    cancel: 'Cancel',
    done: 'Done',
    failed: 'agentq refused: {{output}}',
    reapResult: '{{killed}} terminals closed, {{mb}} MB freed',
    reapPreviewResult: '{{count}} idle terminals ({{mb}} MB) can be closed',
  },

  empty: {
    title: 'No agents yet',
    body: 'When an AI agent starts working here — VS Code Copilot Chat, Copilot CLI, Codai, Codex or Claude Code — it appears on this page.',
  },
  notInstalled: 'agentq is not installed on this machine, so only AI sessions are shown.',
  noHost: {
    title: 'No data arriving',
    body: 'Vitals cannot reach the component that reads agent state. Restarting Vitals usually fixes this.',
  },
  error: {
    title: 'Could not read agent state',
  },
};

const ro = {
  title: 'Agenți AI',
  subtitle: 'Ce agenți AI lucrează pe acest calculator, ce așteaptă și cât costă.',
  live: 'Live',
  updated: 'Actualizat acum {{ago}}',

  tab: {
    overview: 'Prezentare',
    sessions: 'Sesiuni',
    queues: 'Cozi',
    worktrees: 'Worktree-uri',
    jobs: 'Joburi și release-uri',
    journal: 'Jurnal',
    cloud: 'Cloud',
    insights: 'Statistici',
  },

  kpi: {
    working: 'Lucrează acum',
    waiting: 'Așteaptă',
    idle: 'Te așteaptă pe tine',
    building: 'Compilează',
    queued: 'La coadă',
    stale: 'Deținători blocați sau morți',
    jobs: 'Joburi în fundal',
    suspect: 'Necesită atenție',
    leases: 'Worktree-uri ocupate',
    cap: 'Sloturi de build',
  },

  client: {
    vscodeCopilot: 'VS Code Copilot Chat',
    copilotCli: 'Copilot CLI',
    codai: 'Codai',
    codex: 'Codex',
    claudeCode: 'Claude Code',
    other: 'Altele',
  },

  state: {
    working: 'Lucrează',
    waiting: 'Așteaptă',
    idle: 'Te așteaptă pe tine',
    ended: 'Încheiată',
  },

  liveness: {
    alive: 'Activ',
    stale: 'Blocat',
    dead: 'Mort',
    unknown: 'Necunoscut',
  },

  session: {
    started: 'Pornită',
    lastActivity: 'Ultima activitate',
    active: 'Timp activ',
    activeShare: '{{pct}} din sesiune petrecut lucrând',
    turns: 'Ture',
    prompts: 'Mesajele tale',
    tools: 'Apeluri de unelte',
    failures: 'Unelte eșuate',
    tokens: 'Tokeni intrare / ieșire',
    model: 'Model',
    branch: 'Branch',
    nowRunning: 'Rulează {{tool}}',
    topTools: 'Unelte folosite cel mai des',
    agentqWork: 'Prin agentq',
    holds: 'Deține {{resource}}',
    waitsFor: 'Așteaptă {{resource}}',
    lease: 'Worktree {{slot}} pe {{branch}}',
    noWork: 'Nicio coadă agentq sau worktree acum.',
    untitled: 'Sesiune fără titlu',
  },

  queue: {
    capacity: '{{held}} din {{capacity}} ocupate',
    waiting_one: '{{count}} așteaptă',
    waiting_few: '{{count}} așteaptă',
    waiting_other: '{{count}} așteaptă',
    held: 'Rulează',
    waited: 'Așteaptă de {{ago}}',
    heldFor: 'Rulează de {{ago}}',
    lane: 'Bandă {{lane}}',
    yielded: 'Inactiv (și-a cedat slotul)',
    wedged: 'Logul de build a amuțit',
    empty: 'Nicio coadă nu e ocupată.',
    frozen: 'Înghețată',
    blocked: 'Blocată',
  },

  machine: {
    title: 'Sloturi de build pe acest calculator',
    adaptive: 'Se adaptează între {{min}} și {{max}} după CPU și memorie',
    why: 'Ultima decizie: {{why}}',
    cpu: 'CPU (medie)',
    ram: 'Memorie liberă',
    commit: 'Commit liber',
  },

  lease: {
    owner: 'Proprietar',
    alive: 'Proprietarul rulează',
    gone: 'Proprietarul a dispărut',
    stale: 'Expirat — agentq îl va recupera',
    idle: 'Inactiv de {{ago}}',
    empty: 'Niciun worktree nu e închiriat.',
  },

  job: {
    expected: 'Estimat {{min}} min',
    elapsed: 'Rulează de {{ago}}',
    silent: 'Log tăcut de {{ago}}',
    exit: 'Cod ieșire {{code}}',
    state: {
      running: 'Rulează',
      done: 'Terminat',
      failed: 'Eșuat',
      killed: 'Oprit',
      suspect: 'Suspect',
      dead: 'Mort',
      unknown: 'Necunoscut',
    },
    empty: 'Niciun job în fundal în ultima zi.',
    releases: 'Release-uri',
    releasesEmpty: 'Niciun release în ultimele două zile.',
    phase: 'Faza {{phase}}',
  },

  journal: {
    filter: 'Filtrează evenimentele',
    all: 'Toate',
    problems: 'Probleme',
    empty: 'Niciun eveniment nu corespunde.',
  },

  cloud: {
    title: 'Cloud',
    down: '{{ws}} nu e disponibil până la {{until}} — rulările merg local.',
    downReason: 'Motiv: {{reason}}',
    healthy: 'Niciun workspace cloud nu e marcat indisponibil.',
    runs: 'Rulări în cloud în ultimele {{hours}} ore',
    signIn: 'Conectează-te la codai ca să vezi agenții din cloud',
    signInBody:
      'Sesiunile, mediile și consumul din cloud vin din contul tău codai. Nu se trimite nimic până nu te conectezi.',
    signInButton: 'Conectează-te cu codai',
    waiting: 'Se așteaptă aprobarea în browser…',
    code: 'Confirmă acest cod în browser:',
    account: 'Cont codai',
    hosts: 'Desktop-uri conectate',
    noHosts: 'Niciun desktop nu e conectat la codai acum.',
    signOut: 'Deconectează-te',
    sessions_one: '{{count}} sesiune cloud în ultima zi, listată la Sesiuni.',
    sessions_few: '{{count}} sesiuni cloud în ultima zi, listate la Sesiuni.',
    sessions_other: '{{count}} de sesiuni cloud în ultima zi, listate la Sesiuni.',
  },

  insights: {
    window: 'Ultimele {{hours}} ore',
    runs: 'Rulări',
    failures: 'Eșecuri',
    commits: 'Commit-uri',
    waitP50: 'Așteptare tipică',
    waitP90: 'Așteptare lentă (p90)',
    holdP50: 'Build tipic',
    holdP90: 'Build lent (p90)',
    waitTotal: 'Timp petrecut de agenți așteptând',
    perHour: 'Rulări pe oră',
    byRepo: 'Pe repository',
    repoRow: '{{runs}} rulări · {{fail}} eșuate · {{hold}} build · {{wait}} așteptare',
  },

  action: {
    freeze: 'Îngheață',
    clear: 'Deblochează',
    break: 'Elimină tichetul',
    unlease: 'Eliberează worktree-ul',
    killJob: 'Oprește jobul',
    note: 'Lasă o notă',
    reap: 'Închide terminalele inactive',
    reapPreview: 'Previzualizare',
    reason: 'Motiv (îl citesc ceilalți agenți)',
    run: 'Rulează',
    cancel: 'Anulează',
    done: 'Gata',
    failed: 'agentq a refuzat: {{output}}',
    reapResult: '{{killed}} terminale închise, {{mb}} MB eliberați',
    reapPreviewResult: '{{count}} terminale inactive ({{mb}} MB) pot fi închise',
  },

  empty: {
    title: 'Încă niciun agent',
    body: 'Când un agent AI începe să lucreze aici — VS Code Copilot Chat, Copilot CLI, Codai, Codex sau Claude Code — apare pe această pagină.',
  },
  notInstalled: 'agentq nu e instalat pe acest calculator, deci se afișează doar sesiunile AI.',
  noHost: {
    title: 'Nu sosesc date',
    body: 'Vitals nu poate ajunge la componenta care citește starea agenților. Repornirea Vitals rezolvă de obicei problema.',
  },
  error: {
    title: 'Starea agenților nu a putut fi citită',
  },
};

export const bundles = { en, ro } as const;

export function registerAgentsStrings(): void {
  for (const [locale, bundle] of Object.entries(bundles)) {
    i18n.addResourceBundle(locale, AGENTS_NS, bundle, true, false);
  }
}
