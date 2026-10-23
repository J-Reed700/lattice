"""Content pools for the synthetic-library-v3 generator.

Everything here is invented. The setting is one person's vault: Rowan Selke, a
platform engineer at Quillon Grid, who keeps work runbooks, company policy,
vendor paperwork, meeting notes, a journal, reading notes, recipes and house
records in the same library. Names are deliberately non-existent words so a
retrieval score measures retrieval rather than what an embedding model already
memorised about the real world.

The generator composes documents from these pools. Filler must never state a
fact that a query asks for, so nothing here contains an identifier, an amount or
a date that a needle uses; the generator's integrity pass enforces that by
searching the finished corpus.
"""

COMPANY = "Quillon Grid"
OWNER = "Rowan Selke"
PARTNER = "Noor Balestri"
HOUSE = "14 Tanmoor Row"

POOLS = {
    "person": [
        "Marisol Quenby", "Tobin Hafflin", "Dagny Oreskel", "Priya Vantharam",
        "Emeric Losk", "Gretel Umkhov", "Casper Nyewood", "Ilse Brantgard",
        "Rafferty Poole", "Soraya Dunmarch", "Lev Ostrander", "Yusra Kellinghaus",
        "Bram Tiddercombe", "Anneke Voskuil", "Mateo Arrendell", "Halina Prest",
    ],
    "team": [
        "the Merridew platform team", "the Tarrow data group", "the Sepwick field crew",
        "the Nyelle security desk", "the Halloway reliability bench",
    ],
    "product": ["Ostrel", "Pellum", "Garnetline", "Sibbe", "Wrackford", "Dunlark"],
    "service": [
        "ingest-fanout", "probe-registry", "tariff-ledger", "signal-vault",
        "edge-warden", "shard-marshal", "replay-quill", "tenant-broker",
    ],
    "component": [
        "the retry shim", "the quota broker", "the span collator", "the checkpoint reaper",
        "the lease keeper", "the index compactor", "the offset ledger", "the fan-in buffer",
        "the schema latch", "the drain valve", "the replay cursor", "the admission gate",
    ],
    "metric": [
        "p99 write latency", "backlog depth", "lease churn", "shard skew",
        "commit lag", "probe drop rate", "admission rejections", "compaction debt",
    ],
    "env": [
        "staging", "canary", "prod-east", "prod-west", "the Wendover Basin edge ring",
        "the Selkirk North ring",
    ],
    "vendor": [
        "Calderwick Freight", "Tessivar Cloud", "Hobb and Marren Legal",
        "Brackleton Facilities", "Quorvin Metrology", "Aldersgate Print",
        "Renholm Staffing", "Pell and Stourbridge Insurance",
    ],
    "site": ["Wendover Basin", "Selkirk North", "Port Ammerly", "Drummel Flats", "Harrowgate Yard"],
    "weekday": ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"],
    "month": [
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
    ],
    "channel": [
        "the #merridew-oncall room", "the field-ops bridge", "the incident line",
        "the Nyelle escalation queue", "the vendor desk inbox",
    ],
    "artifact": [
        "the deployment manifest", "the drift report", "the capacity worksheet",
        "the audit bundle", "the rotation calendar", "the handover packet",
    ],
    "risk": [
        "a silent data gap", "a duplicate settlement", "an unbounded retry storm",
        "a stale lease takeover", "a partial index rebuild", "a cross-tenant read",
    ],
    "tool": [
        "the console", "the field tablet", "the rollout dashboard", "the query planner",
        "the lease inspector", "the trace viewer",
    ],
    "room": [
        "the kitchen", "the back bedroom", "the utility cupboard", "the loft",
        "the front hall", "the garage", "the side return",
    ],
    "appliance": [
        "the boiler", "the extractor fan", "the dishwasher", "the immersion heater",
        "the washing machine", "the garden tap", "the consumer unit",
    ],
    "ingredient": [
        "smoked paprika", "brown butter", "fermented chilli", "orange zest",
        "toasted barley", "preserved lemon", "buttermilk", "black garlic",
        "wild fennel", "cured egg yolk", "burnt honey", "green peppercorn",
    ],
    "dish": [
        "the barley soup", "the onion tart", "the braised shoulder", "the cold noodle bowl",
        "the buckwheat pancakes", "the tomato confit", "the bean stew",
    ],
    "technique": [
        "a slow dry brine", "a two-stage rest", "a low oven finish", "a quick pickle",
        "a hard sear then a covered braise", "a cold ferment overnight",
    ],
    "author": [
        "Dresnik", "Ollmarch", "Veyne-Dour", "Fabrisse", "Kollmet", "Andreu-Sparr",
        "Tessendorf", "Waybrook",
    ],
    "field": [
        "queueing theory", "estimation under drift", "failure taxonomy",
        "human factors in on-call work", "distributed clock discipline",
        "capacity forecasting", "evidence grading",
    ],
    "concept": [
        "admission pressure", "the blast-radius envelope", "the recovery budget",
        "observational slack", "the handover deficit", "tail correlation",
    ],
    "mood": [
        "restless", "steady", "thin and stretched", "oddly cheerful", "flat",
        "quietly pleased", "wound up",
    ],
    "place": [
        "the towpath", "the long bench by the reservoir", "the market on Ferrol Street",
        "the reading room", "the allotment", "the bus out to Drummel Flats",
    ],
    "plant": [
        "the fig cutting", "the winter chard", "the hedge at the back", "the rosemary",
        "the seedlings on the sill", "the apple espalier",
    ],
    "season": ["late winter", "early spring", "high summer", "the first cold week", "leaf-fall"],
}

# --- sentence templates -------------------------------------------------------
#
# Slots are {name} drawn from POOLS, or {name:lo-hi} for an integer. Numbers here
# are ordinary operational figures; needle values are injected separately and are
# always distinctive enough that a filler number cannot collide with one.

OPS_SENTENCES = [
    "{component} on {service} drains into {env} once {metric} settles under the standing limit.",
    "When {metric} climbs on {product}, check {component} before touching the rollout at all.",
    "{person} keeps {artifact} current for {team}, and the copy in {tool} lags by a cycle.",
    "A restart of {service} in {env} is safe once {component} reports an empty queue.",
    "Do not widen the pool on {product} to work around {risk}; it hides the cause and costs a week.",
    "The {weekday} rollout window for {service} belongs to {team} unless {person} has swapped it.",
    "{component} holds its state in memory, so a crash loses roughly one window of {metric}.",
    "Field crews at {site} see this first because their uplink buffers longer than {env} does.",
    "{person} noted that {component} and {component} disagree about ownership during a split.",
    "Record the quota bucket and the tenant hint in {artifact}; without them the replay is guesswork.",
    "Escalate to {channel} if {metric} stays elevated for two consecutive windows.",
    "{service} degrades gracefully: reads keep serving from the warm copy while {component} rebuilds.",
    "The {month} capacity review moved {product} onto the wider shard map for {site}.",
    "Never run the compaction pass and the lease sweep together; {risk} follows almost every time.",
    "{team} owns the pager for {service}; {team} owns the underlying storage.",
    "A partial drain leaves {component} healthy from the outside, which is why the check reads depth, not status.",
    "{tool} shows {metric} averaged over five minutes, so a short spike will not appear there at all.",
    "After any schema move on {product}, re-run the drift report before you let {env} back into rotation.",
    "The replay path tolerates duplicates; the settlement path does not, and that asymmetry drives the order of steps.",
    "{person} prefers to hold the change until {weekday} rather than ship into a thin on-call week.",
    "Two tenants on {product} share a bucket at {site}, which is a historical accident nobody has unpicked.",
    "Watch {component} for a minute after the cutover: the first window always looks worse than it is.",
    "Cold starts on {service} run to the better part of a minute, which is why the warm pool exists.",
    "The {month} drill found that {team} could not reach {channel} from the field tablet at all.",
    "{artifact} is the record of record; screenshots in chat are not, however convenient they feel.",
]

HR_SENTENCES = [
    "Managers at {COMPANY} are expected to discuss workload with each report at least once a quarter.",
    "A request routed through the wrong queue is not refused; it is re-routed, and the clock restarts.",
    "{person} in the people team keeps the register of approved exceptions for {team}.",
    "Anything agreed in a corridor conversation has to be written down before it takes effect.",
    "Staff based at {site} follow the local calendar for public holidays rather than the head-office one.",
    "The review cycle opens in {month} and closes four weeks later; late submissions carry over.",
    "Where a rule and a local addendum conflict, the addendum wins only inside its named site.",
    "Probation is a two-way arrangement and the same notice terms apply in both directions.",
    "Travel booked outside the approved tool is reimbursed only when the tool was demonstrably unavailable.",
    "A grievance may be raised without naming the other party, though that limits what can be investigated.",
    "Long-service leave accrues from the start date, not from the date the contract was last amended.",
    "Equipment issued to a leaver is collected by {vendor}, not by the manager personally.",
    "Working from another country for more than a fortnight changes the tax position and needs notice.",
    "The learning budget is per person and does not pool across {team}.",
    "Interview panels must include one person from outside the hiring team.",
    "A reference request is answered by the people team only, in the standard form.",
    "Shift swaps at {site} are recorded in the rota, not arranged privately between the two people.",
    "Nobody is asked to be reachable on annual leave, and an on-call handover exists so that they are not.",
    "Expenses submitted more than a quarter after the date are paid at the manager's discretion.",
    "Parental leave entitlement is unaffected by a change of team inside {COMPANY}.",
    "Where a policy is silent, the safer reading is the one that applies until the policy is revised.",
    "Employees may keep a personal copy of anything they wrote themselves, with the customer details removed.",
]

LEGAL_SENTENCES = [
    "Neither party is liable for a delay caused by an event outside its reasonable control.",
    "Notices take effect on delivery to the address recorded in the order schedule.",
    "The supplier warrants that it holds the licences its performance requires at each named site.",
    "Any variation is effective only when signed by an authorised representative of each party.",
    "Charges are stated exclusive of tax, which is added at the prevailing rate on invoice.",
    "The customer may audit the supplier's records once in any twelve-month period on reasonable notice.",
    "Neither party may assign this agreement without the other's written consent, not unreasonably withheld.",
    "Where a schedule conflicts with these terms, the schedule prevails for the service it describes.",
    "The supplier shall keep the customer's confidential information for no longer than the retention schedule allows.",
    "Termination for convenience requires the notice period set out in the order schedule.",
    "Each party shall appoint a contract manager and shall notify the other of any change of appointment.",
    "Service credits are the customer's sole financial remedy for a failure to meet a service level.",
    "The supplier shall not subcontract the whole of the services without prior written consent.",
    "Intellectual property created specifically for the customer vests in the customer on payment.",
    "The parties shall attempt to resolve a dispute at contract-manager level before any escalation.",
    "Insurance shall be maintained for the term and for a period after expiry as set out below.",
    "A waiver of a breach is not a waiver of any later breach of the same provision.",
    "The supplier shall comply with the customer's site rules while present at a customer location.",
    "Invoices are payable within the period stated in the order schedule, without set-off.",
    "This agreement is governed by the law of the place stated in the order schedule.",
]

MEETING_SENTENCES = [
    "{person} walked through the {metric} picture for {product}; nothing new since the last session.",
    "{team} confirmed that the {month} rollout is still tracking to the agreed window.",
    "Carried over: the open question about {component} and whether it needs its own pager rotation.",
    "No change on the {site} uplink work; {vendor} has not come back with a date.",
    "{person} asked whether {risk} is covered by the current drill plan, and it is.",
    "{team} will take the write-up for {artifact} and circulate it before the next session.",
    "The backlog on {service} is flat, which is the intended outcome of the {month} change.",
    "A short discussion about {tool}; the view is that it is adequate and not worth replacing yet.",
    "{person} flagged that the handover notes from {site} are still arriving as photographs.",
    "Nobody raised a blocker for the {weekday} window.",
]

JOURNAL_SENTENCES = [
    "Woke {mood}. Walked as far as {place} before the light went.",
    "Long stretch on {component} today and I still do not trust the failure path.",
    "{PARTNER} made {dish} and we ate it standing up in the kitchen.",
    "Checked {plant}; {season} has been kinder to it than I expected.",
    "Read for an hour and understood maybe half of it, which is the usual ratio.",
    "The house is quiet in a way I have stopped finding unsettling.",
    "Went to {place} and did not think about work for most of it.",
    "Slept badly, then slept too long, and lost the morning to that.",
    "Tried {technique} again with less salt and it was better, though not right.",
    "Wrote three paragraphs, deleted two, kept the one that was actually about something.",
    "{PARTNER} is away until {weekday} and the flat is echoing.",
    "Cold enough that the boiler ran all evening and I could hear it from bed.",
]

READING_SENTENCES = [
    "{author} argues that {concept} is measurable only against a declared budget, never in the abstract.",
    "The chapter on {field} is thin but the appendix is the reason to keep the book.",
    "The worked example is honest about its assumptions, which is rarer than it should be.",
    "{author} keeps returning to the idea that a queue is a decision deferred, not a decision avoided.",
    "I disagree with the framing of {concept}: it treats variance as noise rather than as signal.",
    "Useful distinction here between a control that prevents and a control that only reveals.",
    "The empirical section covers three deployments, which is not enough to generalise from.",
    "{author} is best on {field} and weakest whenever the argument turns organisational.",
    "Marginal note: this maps almost exactly onto the way {team} handles a thin on-call week.",
    "The bibliography points at two older papers that seem more careful than this one.",
]

RECIPE_SENTENCES = [
    "Salt early and let it sit; {technique} does most of the work here.",
    "Keep {ingredient} back until the end or the heat flattens it.",
    "The dough wants a cold rest, and it improves noticeably on the second day.",
    "If the pan is crowded the sear will not happen, so work in two batches.",
    "{ingredient} and {ingredient} together are the whole point of this one.",
    "Taste before the final seasoning; the reduction concentrates more than it looks like it will.",
    "This scales badly above four portions because the pan surface stops being enough.",
    "A splash of the cooking water loosens the sauce without thinning the flavour.",
    "Rest it properly. Cutting early costs more than any ingredient substitution would.",
]

HOME_SENTENCES = [
    "Checked {appliance} in {room}; nothing obviously wrong, noise unchanged.",
    "{vendor} sent someone out and they were here under an hour.",
    "The damp patch in {room} has not grown since {season}, which I am choosing to find reassuring.",
    "Bled the radiators. {room} took the longest and ran brown for a while.",
    "Replaced the filter on {appliance}; the old one was worse than I expected.",
    "Gutters cleared at the back. The front ones need the longer ladder.",
    "Noted for next time: the stopcock is stiff and wants easing before it is ever needed in a hurry.",
    "The draught in {room} is coming from the floor, not the window.",
    "Nothing to report this month beyond the usual settling noises.",
]

CHAT_LINES = [
    "{person}: seeing {metric} creep on {service} again, anyone else?",
    "{person}: not on my side, {env} looks flat",
    "{person}: i'll take it, give me ten",
    "{person}: is {component} supposed to be draining that slowly",
    "{person}: yes if the lease sweep ran, check {artifact}",
    "{person}: ok that explains it",
    "{person}: do we page {team} or wait for the window",
    "{person}: wait. it's within the band",
    "{person}: field tablet at {site} can't reach {channel}, again",
    "{person}: noted, i'll raise it with {vendor}",
    "{person}: thanks, closing this out",
    "{person}: one more thing before we drop",
]

# Headings for deep trees in long handbooks.
HEADING_PARTS = [
    "Scope and application", "Roles and ownership", "Routine operation", "Change control",
    "Failure handling", "Evidence and records", "Site addenda", "Review and revision",
]
HEADING_SECTIONS = [
    "Definitions", "Preconditions", "Standing duties", "Window rules", "Thresholds",
    "Handover", "Escalation path", "Exceptions", "Record keeping", "Verification",
    "Local variation", "Tooling", "Training", "Retirement", "Open questions",
]
HEADING_SUBS = [
    "Normal case", "Degraded case", "Weekend and holiday cover", "Field-only variation",
    "What not to do", "Worked example", "Common mistake", "Notes from the last drill",
]

# --- vocabulary-mismatch pairs ------------------------------------------------
#
# The statement and the question share no content word. {anchor} is a unique
# invented name the integrity pass can search for; {val} is a unique value. The
# decoy is topically adjacent and answers something else; it is planted in a
# different document and graded 0.

PARAPHRASE_FACTS = [
    {
        "host": "policy",
        "statement": "Reimbursement for personal equipment procured without a prior purchase order is "
                     "withheld until the {anchor} committee convenes, which it does every {val}.",
        "question": "I bought a monitor with my own money and nobody signed off first. How long before I hear whether I get that money back?",
        "decoy": "Equipment ordered through the approved catalogue is dispatched from the {site} store within a working week.",
    },
    {
        "host": "policy",
        "statement": "An employee whose role is reclassified retains the {anchor} band for {val} before the "
                     "new band takes effect.",
        "question": "If my job title changes, how long do I stay on my old pay level?",
        "decoy": "A change of line manager does not alter an employee's grade or leave entitlement.",
    },
    {
        "host": "policy",
        "statement": "Absence caused by a dependant's illness is recorded against the {anchor} allowance, "
                     "which is capped at {val} in any rolling year.",
        "question": "My kid was sick and I had to stay home. Is there a limit on that kind of time off?",
        "decoy": "Planned medical appointments are taken as ordinary leave unless the appointment is at the employer's request.",
    },
    {
        "host": "runbook",
        "statement": "A node that fails its liveness probe twice in succession is quarantined by the {anchor} "
                     "guard and is not readmitted for {val}.",
        "question": "What happens to a machine that stops answering health checks, and how soon can it come back?",
        "decoy": "A node that passes every probe but reports high memory pressure is drained slowly rather than removed.",
    },
    {
        "host": "runbook",
        "statement": "Where a replay would write the same settlement twice, the {anchor} latch refuses the "
                     "batch and emits a single reason line carrying {val:code}.",
        "question": "How does the system stop a rerun from paying somebody a second time, and how would I know it did?",
        "decoy": "A replay that finds no matching window exits quietly and leaves no entry in the ledger.",
    },
    {
        "host": "contract",
        "statement": "Should the supplier withdraw from a named site, the {anchor} undertaking obliges it to "
                     "continue at that site for {val} after notice.",
        "question": "If our supplier walks away from one of our locations, are they on the hook to keep going there for a while?",
        "decoy": "The supplier may propose an alternative site, subject to the customer's written agreement.",
    },
    {
        "host": "contract",
        "statement": "Goods rejected on inspection are held at the supplier's risk under the {anchor} term, and "
                     "collection must occur within {val}.",
        "question": "We turned down a delivery because it was damaged. Who is responsible for it while it sits here, and how fast must they take it away?",
        "decoy": "Goods accepted on inspection pass to the customer's risk at the moment of unloading.",
    },
    {
        "host": "runbook",
        "statement": "The {anchor} sweep discards observations whose recorded time precedes the previous "
                     "checkpoint by more than {val}.",
        "question": "How stale can a reading be before the system throws it away instead of storing it?",
        "decoy": "Readings that arrive out of order but within the window are reordered in place before storage.",
    },
    {
        "host": "policy",
        "statement": "Documents bearing the {anchor} marking may not leave a customer site, and a breach is "
                     "reportable within {val}.",
        "question": "There is paperwork I am apparently not allowed to take off a client's premises. If I did by accident, how quickly must that be flagged?",
        "decoy": "Documents marked for internal circulation may be carried between offices but not stored on personal devices.",
    },
    {
        "host": "contract",
        "statement": "Where a price is disputed, the {anchor} provision suspends only the contested portion of "
                     "the invoice, and interest on it runs from {val:date}.",
        "question": "If we argue about part of a bill, do we have to pay the rest, and when does interest start on the bit we are arguing about?",
        "decoy": "An invoice that is not disputed within the stated period is treated as accepted in full.",
    },
    {
        "host": "runbook",
        "statement": "A tenant exceeding its share of the shared bucket is throttled by the {anchor} valve, "
                     "which releases pressure in steps of {val:percent}.",
        "question": "One customer is hogging capacity that everybody shares. How is that reined in, and does it ease off gradually or all at once?",
        "decoy": "A tenant under its share receives no preference; unused capacity is not carried forward.",
    },
    {
        "host": "policy",
        "statement": "Recordings made during a customer call are destroyed under the {anchor} rule after {val} "
                     "unless a hold is in force.",
        "question": "How long do we keep audio from calls with clients before it is wiped?",
        "decoy": "Notes typed during a customer call are retained with the account record for as long as the account is open.",
    },
    {
        "host": "contract",
        "statement": "Under the {anchor} clause the supplier bears the cost of a repeat visit where the first "
                     "attendance failed for reasons within its control, up to {val:amount} per incident.",
        "question": "If the engineer turns up and cannot do the job because of their own mistake, who pays for them to come back, and is there a ceiling on it?",
        "decoy": "A visit cancelled by the customer with less than a day's notice is chargeable at the standard call-out rate.",
    },
    {
        "host": "runbook",
        "statement": "When the clock source disagrees with its peers the {anchor} arbiter freezes ordering and "
                     "holds writes for {val} before failing them outright.",
        "question": "What does the system do when the machines cannot agree on what time it is?",
        "decoy": "A clock that drifts slowly but stays within tolerance is corrected gradually without pausing anything.",
    },
    {
        "host": "policy",
        "statement": "Staff who arrange their own accommodation instead of using the booking tool are paid the "
                     "{anchor} rate of {val:amount} per night, with no receipts required.",
        "question": "I would rather stay with a friend than book a hotel through the system. Is there a flat amount for that?",
        "decoy": "Accommodation booked through the tool is billed centrally and does not appear on a personal expense claim.",
    },
    {
        "host": "contract",
        "statement": "The {anchor} schedule fixes the response target for a total loss of service at {val}, "
                     "measured from the first alert rather than from the ticket.",
        "question": "When everything is down, how fast are they meant to react, and does the countdown start when we raise a ticket or earlier?",
        "decoy": "A partial loss of service carries a longer response target and is measured from the ticket.",
    },
]

# --- multilingual documents ---------------------------------------------------
#
# Short and simple on purpose. Each carries one distinctive fact token so the
# integrity pass can prove the fact appears in exactly one document.

MULTILINGUAL = [
    {
        "id": "intl-de-schluessel",
        "language": "de",
        "title": "Schlüsselordnung Standort Wendover",
        "kind": "facility note",
        "token": "T-2194",
        "text": (
            "Schlüsselordnung Standort Wendover\n\n"
            "Der Ersatzschlüssel für den Technikraum liegt im Tresor T-2194 hinter der Rezeption. "
            "Die Ausgabe erfolgt nur gegen Unterschrift im Schlüsselbuch.\n\n"
            "Verlorene Schlüssel sind am selben Tag der Gebäudeverwaltung zu melden. "
            "Ein Nachschlüssel wird erst nach Freigabe durch die Standortleitung angefertigt.\n\n"
            "Externe Firmen erhalten keinen Dauerschlüssel, sondern eine Tageskarte."
        ),
        "queries": [
            {"lang": "de", "text": "Wo wird der Ersatzschlüssel für den Technikraum aufbewahrt?"},
            {"lang": "en", "text": "Where is the spare key for the plant room at the Wendover site kept?"},
        ],
    },
    {
        "id": "intl-de-lueftung",
        "language": "de",
        "title": "Wartungsplan Lüftungsanlage",
        "kind": "maintenance schedule",
        "token": "alle 94 Tage",
        "text": (
            "Wartungsplan Lüftungsanlage\n\n"
            "Die Filter der Lüftungsanlage werden alle 94 Tage gewechselt. "
            "Der Wechsel wird im Anlagenbuch eingetragen.\n\n"
            "Zwischen zwei Wechseln wird der Differenzdruck wöchentlich abgelesen. "
            "Steigt er deutlich an, wird der Filter vorzeitig getauscht.\n\n"
            "Die Anlage läuft im Winter mit reduzierter Drehzahl."
        ),
        "queries": [
            {"lang": "de", "text": "Wie oft werden die Filter der Lüftungsanlage gewechselt?"},
        ],
    },
    {
        "id": "intl-de-besucher",
        "language": "de",
        "title": "Besucheranmeldung",
        "kind": "facility note",
        "token": "Formular B-77",
        "text": (
            "Besucheranmeldung\n\n"
            "Besucher werden mit dem Formular B-77 angemeldet. "
            "Die Anmeldung muss vor dem Besuchstag eingehen.\n\n"
            "Am Empfang wird ein Besucherausweis ausgegeben, der beim Verlassen zurückzugeben ist. "
            "Besucher werden im Technikbereich stets begleitet."
        ),
        "queries": [
            {"lang": "de", "text": "Mit welchem Formular meldet man einen Besucher an?"},
        ],
    },
    {
        "id": "intl-es-transporte",
        "language": "es",
        "title": "Contrato de transporte con Calderwick",
        "kind": "vendor note",
        "token": "una penalización del 7,5 %",
        "text": (
            "Contrato de transporte con Calderwick\n\n"
            "Si la entrega llega con más de un día de retraso, se aplica una penalización del 7,5 % "
            "sobre el importe del envío.\n\n"
            "El transportista debe avisar del retraso antes de la hora prevista de entrega. "
            "Las incidencias se registran en el parte de recepción.\n\n"
            "La penalización no se aplica cuando el retraso se debe a un cierre del puerto."
        ),
        "queries": [
            {"lang": "es", "text": "¿Qué penalización se aplica si el transportista entrega con retraso?"},
            {"lang": "en", "text": "What penalty applies when the freight carrier delivers late?"},
        ],
    },
    {
        "id": "intl-es-gastos",
        "language": "es",
        "title": "Política de gastos de viaje",
        "kind": "policy",
        "token": "42 euros",
        "text": (
            "Política de gastos de viaje\n\n"
            "Se exige justificante para cualquier gasto superior a 42 euros. "
            "Los gastos menores se declaran en el resumen mensual.\n\n"
            "Las comidas se reembolsan solo durante desplazamientos con pernoctación. "
            "El transporte público tiene preferencia sobre el taxi."
        ),
        "queries": [
            {"lang": "es", "text": "¿A partir de qué importe hace falta justificante de gasto?"},
        ],
    },
    {
        "id": "intl-es-repuestos",
        "language": "es",
        "title": "Inventario de repuestos de Port Ammerly",
        "kind": "inventory note",
        "token": "estante R-14",
        "text": (
            "Inventario de repuestos de Port Ammerly\n\n"
            "La bomba de reserva se guarda en el estante R-14 del almacén frío. "
            "Se revisa una vez por temporada.\n\n"
            "Las juntas y las correas están en el armario contiguo. "
            "Cuando se retira una pieza se anota en la hoja de almacén."
        ),
        "queries": [
            {"lang": "es", "text": "¿Dónde se guarda la bomba de reserva?"},
            {"lang": "en", "text": "Where is the standby pump stored at the Port Ammerly depot?"},
        ],
    },
    {
        "id": "intl-ru-kalibrovka",
        "language": "ru",
        "title": "Инструкция по калибровке датчиков",
        "kind": "runbook",
        "token": "каждые 11 месяцев",
        "text": (
            "Инструкция по калибровке датчиков\n\n"
            "Датчики давления калибруют каждые 11 месяцев. "
            "Результат записывают в журнал калибровки.\n\n"
            "Перед калибровкой прибор выдерживают в помещении не менее часа. "
            "Если отклонение превышает допуск, датчик снимают с линии.\n\n"
            "Эталон хранится в отдельном шкафу."
        ),
        "queries": [
            {"lang": "ru", "text": "Как часто калибруют датчики давления?"},
            {"lang": "en", "text": "How often are the pressure sensors calibrated?"},
        ],
    },
    {
        "id": "intl-ru-dostup",
        "language": "ru",
        "title": "Правила доступа в серверную",
        "kind": "policy",
        "token": "пропуск серии Ж",
        "text": (
            "Правила доступа в серверную\n\n"
            "В серверную допускаются только сотрудники, имеющие пропуск серии Ж. "
            "Вход фиксируется в электронном журнале.\n\n"
            "Подрядчиков сопровождает дежурный инженер. "
            "Двери нельзя оставлять открытыми даже на короткое время."
        ),
        "queries": [
            {"lang": "ru", "text": "Какой пропуск нужен для входа в серверную?"},
        ],
    },
    {
        "id": "intl-ru-postavka",
        "language": "ru",
        "title": "Отчёт о поставке за март",
        "kind": "delivery report",
        "token": "накладной 5108",
        "text": (
            "Отчёт о поставке за март\n\n"
            "Партия пришла по накладной 5108. При приёмке недостачи не выявлено.\n\n"
            "Упаковка двух ящиков была повреждена, содержимое исправно. "
            "Замечание передано поставщику."
        ),
        "queries": [
            {"lang": "ru", "text": "По какой накладной пришла мартовская партия?"},
        ],
    },
    {
        "id": "intl-ja-yobikagi",
        "language": "ja",
        "title": "予備鍵の保管場所",
        "kind": "facility note",
        "token": "金庫 K-58",
        "text": (
            "予備鍵の保管場所\n\n"
            "機械室の予備鍵は受付の金庫 K-58 に保管する。持ち出しには署名が必要である。\n\n"
            "鍵を紛失した場合は当日中に施設担当へ連絡する。合鍵の作成は所長の承認を要する。\n\n"
            "外部業者には一日限りの入館証を渡す。"
        ),
        "queries": [
            {"lang": "ja", "text": "予備鍵"},
            {"lang": "ja", "text": "機械室の予備鍵はどこに保管されていますか。"},
        ],
    },
    {
        "id": "intl-ja-haisenzu",
        "language": "ja",
        "title": "配線図の改訂記録",
        "kind": "reference",
        "token": "第 6 版",
        "text": (
            "配線図の改訂記録\n\n"
            "現在有効な配線図は第 6 版である。旧版は参照用として保管する。\n\n"
            "改訂は現地確認の後に行う。図面番号は変更しない。\n\n"
            "印刷した図面は一年で差し替える。"
        ),
        "queries": [
            {"lang": "ja", "text": "配線図"},
            {"lang": "en", "text": "Which revision of the wiring diagram is currently in force?"},
        ],
    },
    {
        "id": "intl-ja-tenken",
        "language": "ja",
        "title": "三月の点検記録",
        "kind": "maintenance log",
        "token": "三番ポンプ",
        "text": (
            "三月の点検記録\n\n"
            "三番ポンプから軽い振動を確認した。運転には支障がない。\n\n"
            "次回の点検で軸受を見る。記録は保全台帳に残した。\n\n"
            "そのほかの設備に異常はない。"
        ),
        "queries": [
            {"lang": "ja", "text": "三月の点検で振動が見つかったのはどの設備ですか。"},
        ],
    },
    {
        "id": "intl-zh-yanshou",
        "language": "zh",
        "title": "验收码发放规则",
        "kind": "policy",
        "token": "验收码有效期为 15 天",
        "text": (
            "验收码发放规则\n\n"
            "验收码由现场负责人发放，验收码有效期为 15 天。过期后需重新申请。\n\n"
            "一个验收码只能用于一次交付。作废的验收码要在台账上注明原因。\n\n"
            "外部承包商不得转交验收码。"
        ),
        "queries": [
            {"lang": "zh", "text": "验收码"},
            {"lang": "zh", "text": "验收码的有效期是多久？"},
        ],
    },
    {
        "id": "intl-zh-beiyongbeng",
        "language": "zh",
        "title": "备用泵更换记录",
        "kind": "maintenance log",
        "token": "第 4 号库位",
        "text": (
            "备用泵更换记录\n\n"
            "备用泵存放在冷库的第 4 号库位。更换后旧泵送修。\n\n"
            "本次更换用时两小时，未影响生产。密封件同时更换。\n\n"
            "下次检查安排在换季之前。"
        ),
        "queries": [
            {"lang": "zh", "text": "备用泵"},
            {"lang": "en", "text": "Where is the standby pump kept in the cold store?"},
        ],
    },
    {
        "id": "intl-zh-menjin",
        "language": "zh",
        "title": "门禁权限说明",
        "kind": "policy",
        "token": "二级门禁",
        "text": (
            "门禁权限说明\n\n"
            "进入机房需要二级门禁权限。权限由安全组审批。\n\n"
            "临时人员由值班工程师陪同。离职当天收回门禁卡。\n\n"
            "门禁记录保留一年。"
        ),
        "queries": [
            {"lang": "zh", "text": "进入机房需要什么权限？"},
        ],
    },
]


# --- The per-document cap cases (2026-10-23) ------------------------------------
#
# Filler and templates for the "many passages from one document" and "repetitive
# document" dimensions (docs/design/2026-10-23-per-document-cap.md, section 2).
# None of this joins BANNED_SUBSTRINGS: that string screens every anchor the mint
# issues, so growing it would re-mint the anchors of every older document. The
# generator screens the anchors these cases mint against this text instead.

# A sentence may open with one of these; it multiplies the filler's variety, so a
# long document built from a short bank is still not a near-repetitive one.
CAP_OPENERS = [
    "On the {weekday},", "By {season},", "First thing,", "Later on,", "As usual,",
    "For the record,", "Again,", "Before lunch,", "After the rain,", "Towards evening,",
    "In passing,", "Late in {month},", "Without much fuss,", "According to {person},",
]

# Neutral notebook sentences that suit any of the personal or community records.
CAP_SHARED = [
    "the weather turned colder than forecast and stayed grey all afternoon.",
    "{person} rang to ask about the arrangements and was happy with the answer.",
    "a quiet stretch, with nothing worth writing down beyond the usual.",
    "the clocks went back and the evenings closed in all at once.",
    "the road into town was closed for resurfacing, so everything took longer.",
    "there was a power cut for about an hour in the early evening.",
    "a parcel arrived for {person} and is waiting on the hall table.",
    "the light was good enough to work by until well after six.",
    "a run of dry days, the first since {season}.",
    "the notebook went missing for a week and turned up under the car seat.",
]

CAP_FILLER = {
    "home": [
        "the log store by the back door is half full and the top layer is still damp.",
        "{person} came by to borrow the long ladder and stayed for tea.",
        "swept {room} and found where the mice had been getting in behind the skirting.",
        "the path down to the gate needs gravel before {month}, or it will be mud again.",
        "checked the water meter; the reading is in line with last quarter.",
        "the window catch in {room} has worked loose and rattles whenever the wind gets up.",
        "cut back {plant} hard; it will sulk for a month and then come back stronger.",
        "the gutter over the porch overflowed in the downpour; something is blocking the outlet.",
        "nothing much to report beyond a slow drip from the outside tap.",
        "the smoke alarm on the landing chirped twice overnight, so the battery is going.",
        "a frost on {weekday} night took the tops off the dahlias.",
        "the lane flooded at the dip again and the bin lorry would not come down.",
        "aired {room} for the afternoon; the musty smell is mostly gone.",
        "the shed door has swollen and needs planing before it will shut properly.",
        "the neighbour's cat has taken to sleeping on the coal bunker lid.",
        "the kindling basket is empty again; split a barrow-load from the old fence posts.",
    ],
    "garden": [
        "the borage is covered in bees from mid-morning onwards.",
        "the hawthorn at the bottom hedge came into flower a week early.",
        "smoker fuel running low; collected dry grass and hessian scraps from the shed.",
        "wasps worrying the entrances of the end hives, so the entrance blocks went in.",
        "the ivy on the north wall is the last big forage of the year.",
        "{person} asked about buying a jar for the church fair.",
        "the scales under the end hive read about the same as last week.",
        "cleaned the spare brood boxes with a blowlamp and stacked them in the garage.",
        "the water bath by the hives needs topping up every other day in this heat.",
        "the bramble patch by the stile is full of foragers whenever the sun is out.",
        "the veil zip is going; ordered a replacement before it fails mid-inspection.",
        "rain all of {weekday}, so no inspection; the bees stayed in and sulked.",
        "the oilseed rape across the lane finished flowering this week.",
        "drones are being turned out at the entrances, which means the season is turning.",
        "{plant} is flowering again and the bees have found it.",
    ],
    "community": [
        "the minutes of the last meeting were agreed as a true record.",
        "{person} reported that the hall was booked every weekend in {month}.",
        "the raffle raised enough to cover the new tea urn, with a little over.",
        "a resident asked whether the car park could be resurfaced before {season}.",
        "the treasurer noted that the reserve is healthy but should not be run down.",
        "{person} thanked the volunteers who ran the jumble sale.",
        "the noticeboard outside the hall needs a new lock; the key has gone missing.",
        "it was agreed to write to the bus company about the cancelled evening service.",
        "the youth club would like the back room on {weekday} evenings.",
        "the litter pick on {weekday} morning had twice the usual turnout.",
        "the heating timer was reset after complaints that the hall was cold for the toddler group.",
        "{person} will circulate the draft rota before the next meeting.",
        "there was some discussion of parking at the school gate, with no decision.",
        "the piano tuner is booked for {month}, once the damp has gone.",
        "the defibrillator outside the shop was checked and its pads are in date.",
        "a request was made for a bench by {place}, to be looked into.",
    ],
    "history": [
        "the river froze hard enough for skating below the bridge for most of {month}.",
        "a fair was held on the green and the takings went to the almshouses.",
        "the parish register for this period is water-damaged and several pages are lost.",
        "the harvest was poor, and the price of bread rose sharply by {season}.",
        "the new schoolroom opened with forty pupils and one teacher.",
        "a fire in the bakehouse on the high street spread to two cottages before it was put out.",
        "the vicar's diary mentions a visiting preacher who drew a crowd from three villages.",
        "the toll on the turnpike road was raised, and the carters protested at the gate.",
        "the census book lists most households on the street as weavers or labourers.",
        "a smallpox scare closed the school for most of {season}.",
        "the bridge was widened, and the old stone parapet was rebuilt with the same blocks.",
        "the estate sold its outlying farms in a single auction at the inn.",
        "a letter in the county archive complains about the state of the lanes after the floods.",
        "the church bells were recast and rehung in time for the {month} festival.",
        "the photograph in the album is undated, but the shopfronts suggest the same decade.",
        "the archive catalogue lists the box but not its contents, so it has to be seen in person.",
    ],
    "collections": [
        "the store dehumidifier was emptied and its log sheet initialled.",
        "a pest trap in the textile store caught two clothes moths; the traps were renewed.",
        "light levels in the north gallery were measured and are within the agreed limit.",
        "{person} photographed the frame corners before anything was handled.",
        "acid-free tissue is running low; reorder before the next rotation.",
        "the loan request from the county museum was declined on condition grounds.",
        "a visitor reported a loose label in the east gallery; it was refixed the same day.",
        "handling gloves replaced; the old nitrile stock had perished in the heat.",
        "the volunteer team rolled two rugs onto new tubes in the basement store.",
        "the condition survey of the costume collection is about a third complete.",
        "the window blinds in the long gallery were lowered for the summer.",
        "{person} catalogued the box of samplers that came in with the bequest.",
        "the frame of the oil painting on the stairs shows a hairline crack; monitoring it.",
        "dust covers on the stored furniture were washed and refitted.",
        "the gallery attendants asked for a stool by the door of the long room.",
    ],
    "bench": [
        "the balance was recalibrated against the reference masses before the session.",
        "the drying oven ran hot this morning; the thermostat was reset.",
        "{person} borrowed the sieve stack for the first-year practical.",
        "labels on the old sample bags are fading, so the whole tray was re-labelled.",
        "the fume cupboard alarm tripped twice; facilities have been told.",
        "repeated the blank, and the reading matched the morning's within error.",
        "crucibles washed, dried and stacked by number.",
        "the hydrometer jar was cracked, so the spare came out of the cupboard.",
        "logged the room temperature at the start and the end, as the method requires.",
        "the deionised water ran out mid-afternoon; fetched more from the second floor.",
        "the field sheets were transcribed before the pencil smudged any further.",
        "the muffle furnace needs a full day to cool before it can be opened.",
        "met {person} about the sampling plan for the spring term.",
        "spilled a little of the dispersant; wiped it up and logged it.",
        "the shared bench was left cluttered again; a polite note went up.",
    ],
    "sky": [
        "transparency was poor, with high cloud drifting in from the west all evening.",
        "the dew heater kept the corrector plate clear until about midnight.",
        "the neighbour's security light came on twice and ruined dark adaptation.",
        "collimation checked on a bright star; slightly off, and corrected.",
        "the moons of Jupiter sat in a neat line, with one about to transit.",
        "seeing steadied after midnight and the double star split cleanly.",
        "the mount lost its alignment when the cable snagged; re-aligned on two stars.",
        "a satellite flare crossed the field during the exposure.",
        "packed up early, with frost forming on the tube by eleven.",
        "the Orion nebula showed more structure than last month in the same eyepiece.",
        "tried the new filter on the planetary nebula; a modest improvement.",
        "recorded the sky brightness with the meter pointed at the zenith.",
        "{person} came round with a flask and stayed for the meteor shower.",
        "the finder scope has drifted out of line with the main tube again.",
        "the star chart got damp and the pages for the autumn sky have stuck together.",
    ],
    "travel": [
        "moored above the lock for the night, and the towpath was quiet after dark.",
        "filled the water tank at the services and emptied the cassette.",
        "a heron followed us for a mile, landing ahead each time we caught up.",
        "the engine ran warm on the long pound; the weed hatch was full of plastic.",
        "{person} worked the gates while the boat went in on tickover.",
        "rain from mid-morning, clearing enough by four to dry the roof.",
        "passed a hire boat aground on the bend and gave them a tow off.",
        "bought eggs and bread from the farm shop by the bridge.",
        "a swing bridge was stiff and needed two of us on the beam.",
        "the pub at the junction no longer does food on {weekday}s.",
        "counted eleven boats on the visitor moorings by teatime.",
        "the stern gland is dripping more than it should; tightened it half a turn.",
        "a long tunnel today, with the headlamp on and the horn sounded at both portals.",
        "wind across the reservoir section made steering hard work.",
        "the cratch cover leaks at one corner and the cushions under it got wet.",
    ],
    "kitchen": [
        "use a scale rather than cups; flour volume varies too much with how it is packed.",
        "the oven runs hot at the back, so turn the tray halfway through.",
        "a tin of water in the bottom of the oven gives the crust its shine.",
        "this freezes well sliced, and toasts straight from the freezer.",
        "{ingredient} works in place of the seeds if the pantry is short.",
        "knead until the dough passes the windowpane test, and not much beyond.",
        "the dough is ready when a floured finger leaves a dent that springs back slowly.",
        "a cooler kitchen in {season} slows everything; allow more time rather than more yeast.",
        "score with a single confident cut; hesitating tears the skin.",
        "let the loaf cool completely before slicing or the crumb turns gummy.",
        "this was the version made most often for Sunday lunch.",
        "stir the flour in with the handle of a wooden spoon to keep it from clumping.",
    ],
    "agreement": [
        "Plot holders shall keep their plots in a reasonable state of cultivation.",
        "No building or shed may be erected on a plot without written consent.",
        "Paths between plots shall be kept clear of tools, hoses and produce.",
        "The council may enter the site at any reasonable time to inspect it.",
        "No livestock other than hens may be kept, and no cockerels at all.",
        "Water from the troughs is for watering only and may not be used for washing vehicles.",
        "Rubbish shall be removed from the site and not burned on it.",
        "A plot left uncultivated for a full season may be offered to the next applicant.",
    ],
}

# Headings the filler builder interleaves, one template per shape.
CAP_AGREEMENT_HEADINGS = [
    "The site", "Plots and paths", "Water", "Structures", "Rent and accounts",
    "Inspection", "Termination", "Notices", "Members", "Events on the site",
]
CAP_RECIPE_HEADINGS = [
    "A plain loaf", "The seeded loaf", "Rye and caraway", "Flatbreads", "Sweet buns",
    "Crackers", "A tin loaf for sandwiches", "Pizza bases", "Crumpets", "Pancakes",
]

# Every repeated form also carries its organisation's records note and its standard
# instructions, as real forms do.
CAP_RECORDS_NOTE = (
    "Records note. {org} keeps these sheets for six years, filed in date order, and shows "
    "them to any committee member who asks. If a sheet is damaged or lost, a note saying so "
    "goes in its place, signed by two members. The originals are scanned once a year and the "
    "scans kept on the shared drive. Questions about the records go to the secretary, who also "
    "keeps the list of who holds keys."
)
CAP_FORM_NOTE = (
    "Using this sheet. Fill it in on the day, in ink, and do not leave a line blank: write "
    "\"not checked\" and the reason if something could not be done. Corrections are crossed "
    "through with a single line and initialled, never covered over. If a check fails, the "
    "person who found it tells {org} the same day and writes what was done about it on the back "
    "of the sheet. Sheets are handed to the secretary at the end of the month."
)

# Repetitive documents: one template repeated ten times, one field changing per
# repeat. `header`, `body` and `footer` are identical in every repeat; `field`
# carries `{n}` (the repeat number) and `{value}` (a minted token).
CAP_REPEATED = [
    {
        "slug": "minutes-scan",
        "org": "the residents' association",
        "title": "Residents' association minutes, scanned bundle",
        "kind": "scanned minutes",
        "value": "amount",
        "header": "HOLLY ROW RESIDENTS' ASSOCIATION -- MINUTES OF THE MONTHLY MEETING -- "
                  "scanned from the paper file",
        "body": [
            "Present: the chair, the secretary, the treasurer and members as listed on the "
            "attendance sheet. Apologies were received and noted. The minutes of the previous "
            "meeting were read, agreed as a true record and signed by the chair. Matters "
            "arising: none that are not covered under the standing items below.",
            "Standing items. 1. Correspondence received since the last meeting was tabled and "
            "is filed with these minutes. 2. The maintenance report on the communal garden, the "
            "bin store and the entrance lighting was received without comment. 3. The treasurer "
            "counted the meeting float in front of two members, as the constitution requires, and "
            "the count is recorded below. 4. Members were reminded that complaints about "
            "neighbours are not business for this meeting and should go to the managing agent.",
            "The association meets on the first {weekday} of the month in the community room. Any member may add an item to the agenda by giving it to the secretary seven days before the meeting. Items raised on the night are taken under any other business only if the chair agrees.",
            "Membership. Every household in the row is a member without needing to apply, and one vote is cast per household at a general meeting. The committee is elected each spring for a year; anyone may stand. Subscriptions are voluntary and are spent only on the communal garden, the summer party and the costs of running the association. The accounts are examined by an independent member before the annual meeting and are open to any household that asks.",
        ],
        "field": "Page {n}. Float counted and handed to the treasurer: {value}.",
        "footer": "Signed as a true record by the chair. Retain for six years. Scanned copy; "
                  "the original is in the association's file box.",
        "variant_question": "In the scanned residents' association minutes, how much was the "
                            "float on page 7?",
        "short": {
            "id": "note-float-handover",
            "title": "Treasurer handover note",
            "text": [
                "Handover note for whoever takes over as treasurer of the residents' "
                "association. The meeting float lives in the grey cash tin and is counted at "
                "every meeting in front of two members.",
                "The {anchor} account that holds the float between meetings is being closed "
                "on {value}; after that the float stays in the hall safe and the chair keeps "
                "the key.",
            ],
            "value": "date",
            "question": "When does the residents' association stop keeping the meeting float "
                        "in a bank account?",
        },
    },
    {
        "slug": "coldroom-checklist",
        "org": "the food bank",
        "title": "Food bank cold room, weekly checklist",
        "kind": "checklist",
        "value": "percent",
        "header": "FOOD BANK -- COLD ROOM WEEKLY CHECKLIST -- complete every week, file in "
                  "the blue folder",
        "body": [
            "[x] Door closes and seals on its own when released.\n[x] Thermometer read at "
            "opening and at close, and both readings entered on the wall chart.\n[x] No food "
            "stored on the floor; crates on the racking only.\n[x] Dated labels on every crate; "
            "anything past its date removed and logged on the waste sheet.\n[x] Spill kit "
            "present and complete.\n[x] Fan grille free of ice, drip tray emptied.",
            "Volunteers must not prop the cold room door open while loading. If the alarm "
            "sounds, close the door, wait ten minutes and read the thermometer again before "
            "phoning the duty supervisor.",
            "The cold room is for chilled donations only: dairy, cooked meats, ready meals and fresh produce that will not keep on the shelves. Frozen items go to the chest freezers in the back corridor. Nothing goes back into the cold room once it has left it for a client parcel.",
            "Cleaning. The cold room floor is mopped at the end of every session with the blue mop and the food-safe cleaner, never with bleach. The racking is wiped down weekly, top shelf first, and the door handle and the light switch are wiped at every session. Any spill of raw meat juice is cleaned at once and noted on the waste sheet with the time it was found and who cleaned it. The door curtain strips are washed monthly.",
        ],
        "field": "Week {n}: lowest overnight humidity logged in the cold room: {value}.",
        "footer": "Volunteer signature and supervisor countersignature required.",
        "variant_question": "What was the lowest humidity the food bank's cold room recorded "
                            "in week 7?",
        "short": {
            "id": "note-coldroom-seal",
            "title": "Cold room door seal replaced",
            "text": [
                "{person} fitted a new door seal on the food bank's cold room after the old "
                "one split along the hinge side.",
                "The {anchor} gasket has to be checked again after {value}, and the old seal "
                "stays in the store cupboard until then in case the new one is the wrong size.",
            ],
            "value": "calendar days",
            "question": "After the cold room door seal was replaced at the food bank, how long "
                        "before it gets checked again?",
        },
    },
    {
        "slug": "allotment-checklist",
        "org": "the allotment society",
        "title": "Allotment site weekly checklist",
        "kind": "checklist",
        "value": "date",
        "header": "ALLOTMENT SITE -- WEEKLY SITE CHECK -- site rep to complete and pin to "
                  "the shed door",
        "body": [
            "Gates locked at dusk and the combination changed if anyone has left the society. "
            "Water troughs checked for leaks and the ballcocks freed. Communal shed tidied and "
            "the shared tools returned to their hooks. Paths walked for trip hazards.",
            "The communal skip is for green waste, broken pots and old netting. Plot holders "
            "must not leave rubbish beside the skip when it is full; take it home and bring it "
            "back after the collection.",
            "Site reps rotate monthly. The outgoing rep hands over the padlock list, the water meter reading and this checklist to the incoming rep at the shed. Any vacant plot is reported to the lettings secretary the same week, with a note of its condition.",
            "Water. The troughs are filled from the mains between March and October and drained over winter. Hoses may be used only to fill a can, never left running on a plot, and a sprinkler is not allowed at all. Leaks go to the site rep the same day. In a hosepipe ban the troughs stay on, but watering is by can only and the site rep may close them in the afternoon if pressure drops in the houses nearby.",
        ],
        "field": "Week {n}: next skip collection booked for {value}.",
        "footer": "Problems to the site rep in person or on the noticeboard by the shed.",
        "variant_question": "On the allotment site's weekly checklist, which date was the skip "
                            "collection booked for in week 7?",
        "short": {
            "id": "note-skip-timber",
            "title": "Allotment skip, treated timber",
            "text": [
                "Reminder from the site committee about the communal skip at the allotments.",
                "Treated timber may not go in the skip. Under the {anchor} rule anyone who puts "
                "it there pays for a separate collection, which costs {value}.",
            ],
            "value": "amount",
            "question": "What happens if someone throws treated wood into the allotment skip?",
        },
    },
    {
        "slug": "fire-checks",
        "org": "the village hall committee",
        "title": "Village hall fire safety check sheet",
        "kind": "checklist",
        "value": "date",
        "header": "VILLAGE HALL -- MONTHLY FIRE SAFETY CHECK -- the responsible person "
                  "completes this sheet",
        "body": [
            "Fire exits unobstructed inside and out, and the doors open freely from the inside. "
            "Emergency lighting tested with the key switch and every fitting lit. One call point "
            "tested in rotation and the alarm heard in the main hall, the kitchen and the back "
            "room. Extinguishers in place with pins and seals intact. Fire blanket in the "
            "kitchen, still in its holder.",
            "Hirers are shown the exits and the assembly point at the start of every booking. "
            "Any fault goes in the logbook the same day and is reported to the hall committee.",
            "The hall holds a fire drill twice a year with whichever group has the booking that evening. The responsible person records how long the building took to clear and any door that stuck. The fire risk assessment is reviewed every year by the hall committee.",
            "Kitchen. The cooker is isolated at the wall switch at the end of every hire. Fat is never poured down the sink, and the extractor hood filter is washed monthly. Hirers using the urn must stay in the kitchen while it boils. Candles are allowed only on tables, in holders, and away from the curtains, and nothing is pinned to the ceiling tiles. Smoking and vaping are not allowed anywhere in the building or by the doors.",
        ],
        "field": "Check {n}: extinguisher service next due on {value}.",
        "footer": "Keep this sheet with the fire logbook in the kitchen drawer.",
        "variant_question": "According to the village hall fire check sheet, when was the "
                            "extinguisher service due at check 7?",
        "short": {
            "id": "note-fire-assembly",
            "title": "Village hall fire assembly point moved",
            "text": [
                "The village hall committee has agreed a change to the fire procedure.",
                "The assembly point moves from the car park to {anchor} Green, starting on "
                "{value}, because the car park is being used for the market.",
            ],
            "value": "date",
            "question": "Where do people gather now during a fire drill at the village hall, "
                        "and from when?",
        },
    },
    {
        "slug": "swim-sheet",
        "org": "the swim club",
        "title": "Swim club poolside session sheet",
        "kind": "checklist",
        "value": "amount",
        "header": "SWIM CLUB -- POOLSIDE SESSION SHEET -- the session lead keeps this on the "
                  "clipboard",
        "body": [
            "Lifeguard on the poolside before the first swimmer entered the water. Lane ropes "
            "and backstroke flags in place. First aid box checked and the bag valve mask "
            "sealed. Register taken and matched against the swimmers in the water.",
            "Parents wait in the gallery, not on the poolside. Subs are collected at the "
            "poolside table and counted with a second adult before the cash bag is sealed for "
            "the leisure centre safe.",
            "Swimmers are grouped by ability, not age, and move groups only when the head coach agrees. Galas and county events are announced on the club noticeboard by the pool entrance. Lost property is kept for a month in the leisure centre office.",
            "Coaching. Every coach on the poolside holds a current qualification and a safeguarding certificate, and no coach is ever alone with a swimmer in the changing rooms. Sessions start with a dryland warm-up in the sports hall and end with a cool-down swim. Coaches record attendance on the club app as well as on this sheet, and note any swimmer who leaves the water early and why.",
        ],
        "field": "Session {n}: subs collected at the poolside: {value}.",
        "footer": "Incidents go in the leisure centre's accident book as well as on this sheet.",
        "variant_question": "How much did the swim club collect in subs at session 7?",
        "short": {
            "id": "note-swim-gala",
            "title": "Swim club gala lane hire",
            "text": [
                "Note for the swim club committee about the spring gala.",
                "Lane hire for the {anchor} gala costs {value}, paid to the leisure centre in "
                "advance; it is not refunded if the gala is cancelled.",
            ],
            "value": "amount",
            "question": "How much does the swim club pay the leisure centre for the lanes at "
                        "the gala?",
        },
    },
    {
        "slug": "boat-handover",
        "org": "the boatyard",
        "title": "Boat hire daily handover sheet",
        "kind": "checklist",
        "value": "percent",
        "header": "DAY BOAT HIRE -- HANDOVER SHEET -- complete with the hirer before the boat "
                  "leaves the wharf",
        "body": [
            "Hull walked round for damage and photographs taken. Bilge pumped dry. Life jackets "
            "counted: six adult, two child. Mooring lines coiled on the roof. Horn, tiller pin "
            "and windlasses checked.",
            "Hirer briefed on the locks, the swing bridge and the speed limit. The boat must be "
            "back at the wharf by five; late returns are charged by the hour.",
            "Hirers must not take the boat beyond the junction, must not moor overnight, and must not let anyone under twelve steer without an adult beside them. The boatyard telephone is answered from eight until six, and the engineer can reach most of the route within the hour.",
            "Engine. The boat has a diesel engine with a stern gear and a weed hatch. Hirers must never open the weed hatch with the engine running and must stop the engine before anyone goes into the water. The cooling water tell-tale should run while the engine is on; if it stops, hirers moor up and ring the boatyard. Oil and coolant are checked by the boatyard each morning, not by the hirer.",
        ],
        "field": "Day {n}: fuel left in the tank at handover: {value}.",
        "footer": "Hirer and boatyard both sign; the boatyard keeps the top copy.",
        "variant_question": "On the boat hire handover sheet, how much fuel was left in the "
                            "tank on day 7?",
        "short": {
            "id": "note-boat-stoppage",
            "title": "Boat hire, winter lock stoppage",
            "text": [
                "Notice for the day boat hire crew.",
                "The {anchor} lock on the usual hire route is shut for {value} of winter "
                "repairs, so hirers must turn at the junction instead.",
            ],
            "value": "calendar days",
            "question": "How long is the lock on the boat hire route closed for winter repairs?",
        },
    },
]

# A saved thread where every reply re-quotes everything before it. It floods a
# topic without one changed field, so it has a flooding-guard question only.
CAP_THREAD = {
    "slug": "coach-thread",
    "title": "Saved email thread: coach for the choir tour",
    "kind": "email thread",
    "subject": "Re: coach for the choir tour",
    "replies": [
        "Can we confirm the coach for the choir tour? We need a vehicle for about forty "
        "singers plus the music boxes, leaving from the church.",
        "Forty is fine but the boxes worry me. Could we ask whether the coach has a "
        "proper luggage hold rather than a boot?",
        "Asked. The coach has a full luggage hold. They want final numbers a week before.",
        "Final numbers are hard while the altos are still deciding. Can we give a range?",
        "A range is fine for now. They also asked whether we need a stop on the way.",
        "One stop at the services, please; the drive is long for the older members.",
        "Noted. They suggested leaving half an hour earlier to allow for the stop.",
        "Half an hour earlier means the choir arrives at the church at seven. Is that too "
        "early for everyone?",
        "Seven is early but workable. Can the coach wait at the venue during the concert?",
        "Yes, the driver waits and the return leaves straight after the last piece.",
    ],
    # The organisation's standard email footer, re-quoted with everything else.
    "footer": "This message and anything attached to it are for the choir committee and the "
              "people copied in. If it reached you by mistake, please tell the sender and delete "
              "it. The choir is a registered charity run by volunteers; replies may take a few "
              "days outside the concert season.",
    "short": {
        "id": "note-coach-deposit",
        "title": "Choir tour coach deposit",
        "text": [
            "Choir committee note on paying for the tour coach.",
            "The deposit is due to {anchor} Coaches by {value}; the balance is paid to the "
            "driver on the day of the tour.",
        ],
        "value": "date",
        "question": "By when does the choir have to pay the deposit for the tour coach?",
    },
}
