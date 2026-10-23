#!/usr/bin/env python3
"""Build evals/retrieval/section-context-v1.json.

Two long non-English, non-software documents: a German cell-biology course handbook and a
Spanish parish chronicle. Each "pointing" needle is a clause that names a topic and defers its
values ("siehe nächster Abschnitt", "como se detalla a continuación"); the values sit in the
passage after it. Placement is checked against the indexer's chunker (800 tokens, 120 overlap,
sentence boundaries) under the embedding model's own tokenizer: the pointing clause lies in
passage i and not in passage i+1, the answer lies in passage i+1 and in neither i nor i+2.
Control needles state topic and value in one sentence inside one passage.

usage: build_section_context_v1.py TOKENIZER_JSON OUT_JSON
"""
import itertools
import json
import random
import sys

from tokenizers import Tokenizer

MAX_TOKENS, OVERLAP = 800, 120
SEED = 20261023


def chunk_bounds(tokenizer, text):
    """`SemanticChunker::chunk_text`, returning UTF-8 byte ranges."""
    encoding = tokenizer.encode(text, add_special_tokens=False)
    char_to_byte = [0]
    for ch in text:
        char_to_byte.append(char_to_byte[-1] + len(ch.encode('utf-8')))
    offsets = [(char_to_byte[a], char_to_byte[b]) for a, b in encoding.offsets]
    raw = text.encode('utf-8')
    n = len(offsets)

    def sentence_boundary(start, target):
        min_end = start + OVERLAP + 1
        if target <= min_end:
            return None
        window = min(50, target - min_end)
        if window == 0:
            return None
        for i in range(target - 1, target - window - 1, -1):
            end = offsets[i][1]
            if end <= 0 or end > len(raw):
                continue
            try:
                before = raw[:end].decode('utf-8')
            except UnicodeDecodeError:
                continue
            if before and before[-1] in '.!?\n':
                after = raw[end:].decode('utf-8', errors='ignore')
                if not after or after[0].isspace():
                    if i + 1 >= min_end:
                        return i + 1
        return None

    bounds, start = [], 0
    while start < n:
        end = min(start + MAX_TOKENS, n)
        actual = end
        if end < n:
            actual = sentence_boundary(start, end) or end
        start_byte = offsets[start][0]
        end_byte = offsets[actual - 1][1] if actual < n else len(raw)
        bounds.append((start_byte, end_byte))
        if actual >= n:
            break
        start = actual - OVERLAP
    return bounds


def holders(bounds, span):
    """Passages that wholly contain the byte span."""
    return [i for i, (a, b) in enumerate(bounds) if a <= span[0] and span[1] <= b]


def touches(bounds, span):
    return [i for i, (a, b) in enumerate(bounds) if a < span[1] and span[0] < b]


class Builder:
    def __init__(self, tokenizer, heading, filler):
        self.tokenizer = tokenizer
        self.parts = [heading]
        self.filler = filler
        self.spans = {}

    def text(self, parts=None):
        return '\n\n'.join(parts if parts is not None else self.parts) + '\n'

    def add(self, part, key=None):
        start = len(self.text().encode('utf-8')) - 1 + 2
        self.parts.append(part)
        if key:
            self.spans[key] = (start, start + len(part.encode('utf-8')))

    def add_filler(self, count):
        for _ in range(count):
            self.add(next(self.filler))

    def pointing(self, key, pointer, answer):
        """Place `pointer`, a bridge of filler, then `answer`, so the answer lies in the passage
        after the pointer's and in no other. The lead before the pointer and the bridge after
        it grow one filler part at a time and the first fit is kept; a pointer that straddles
        two passages ends that lead, and a bridge that has pushed the answer past the next
        passage ends that bridge."""
        for lead in itertools.count():
            for bridge in itertools.count():
                trial = list(self.parts)
                trial.extend(self.peek(lead))
                p_start = len(self.text(trial).encode('utf-8')) + 1
                trial.append(pointer)
                p_span = (p_start, p_start + len(pointer.encode('utf-8')))
                trial.extend(self.peek(bridge, skip=lead))
                a_start = len(self.text(trial).encode('utf-8')) + 1
                trial.append(answer)
                a_span = (a_start, a_start + len(answer.encode('utf-8')))
                trial.extend(self.tail(lead + bridge))
                bounds = chunk_bounds(self.tokenizer, self.text(trial))
                p_in, a_in = touches(bounds, p_span), touches(bounds, a_span)
                if len(p_in) != 1:
                    break
                if a_in == [p_in[0] + 1] and holders(bounds, a_span) == a_in:
                    self.add_filler(lead)
                    self.add(pointer, key + ':pointer')
                    self.add_filler(bridge)
                    self.add(answer, key)
                    return
                if max(a_in) > p_in[0] + 1:
                    break

    def tail(self, skip):
        """The filler that follows a trial placement, at least a passage of it, so the
        passages around the placement end where they will."""
        parts, tokens = [], 0
        while tokens <= MAX_TOKENS:
            part = self.peek(1, skip=skip + len(parts))[0]
            parts.append(part)
            tokens += len(self.tokenizer.encode(part, add_special_tokens=False).ids)
        return parts

    def peek(self, count, skip=0):
        """The next `count` filler parts after `skip`, without consuming them."""
        ahead = self.filler.peek(skip + count)
        return ahead[skip:]


class Filler:
    def __init__(self, make):
        self.make = make
        self.buffer = []

    def peek(self, count):
        while len(self.buffer) < count:
            self.buffer.append(self.make())
        return self.buffer[:count]

    def __next__(self):
        self.peek(1)
        return self.buffer.pop(0)


# ---------------------------------------------------------------- German handbook
DE_OBJECTS = ['Zwiebelwurzelspitze', 'Hyazinthenwurzel', 'Ackerbohnenwurzel', 'Knoblauchwurzel',
              'Lilienanthere', 'Weizenwurzel', 'Erbsenkeimling', 'Roggenwurzel']
DE_FIXATIVES = ['Carnoy-Lösung', 'Methanol-Eisessig', 'Ethanol-Eisessig', 'gepuffertem Formalin']
DE_STAINS = ['Orcein', 'Feulgen-Reagenz', 'Toluidinblau', 'Giemsa-Lösung']
DE_GROUPS = ['Gruppe A', 'Gruppe B', 'Gruppe C', 'Gruppe D', 'Gruppe E']
DE_REMARKS = [
    'Die Metaphaseplatten lagen gut gespreizt und ließen sich einzeln zählen.',
    'Anaphasen waren selten, Telophasen dagegen auffällig häufig.',
    'Das Präparat war stellenweise zu dick gequetscht; nur der Rand war auswertbar.',
    'Die Färbung fiel blass aus, die Chromosomen waren aber noch erkennbar.',
    'Im Zentrum der Wurzelspitze lagen die meisten Teilungsstadien dicht beieinander.',
    'Mehrere Zellen zeigten verklumpte Chromosomen, vermutlich ein Fixierungsfehler.',
    'Luftblasen unter dem Deckglas verdeckten etwa ein Drittel des Gesichtsfelds.',
    'Die Zellwände traten deutlich hervor, das Cytoplasma blieb fast farblos.',
    'Prophasen ließen sich nur schwer von Interphasekernen unterscheiden.',
    'Die Gruppe hat das Präparat zur Kontrolle ein zweites Mal angefertigt.',
]
DE_NOTES = [
    'Hinweis zur Arbeitsweise: Pipettenspitzen werden nach jedem Reagenz gewechselt, auch wenn '
    'die Flasche gleich aussieht.',
    'Hinweis zur Dokumentation: Jedes Präparat erhält eine fortlaufende Nummer, die auf dem '
    'Objektträger und im Protokollbogen übereinstimmen muss.',
    'Hinweis zum Mikroskopieren: Zuerst mit dem schwächsten Objektiv scharf stellen, erst dann '
    'zum Ölimmersionsobjektiv wechseln.',
    'Hinweis zur Ordnung: Benutzte Objektträger kommen in die Glasbox, nicht in den Restmüll.',
    'Hinweis zum Protokollbogen: Unklare Stadien werden als unklar notiert, nicht geschätzt.',
    'Hinweis zur Zusammenarbeit: Zwei Personen zählen dasselbe Gesichtsfeld unabhängig, '
    'Abweichungen werden besprochen.',
]


def german_filler(rng):
    counter = iter(range(1, 10_000))

    def make():
        if rng.random() < 0.18:
            return rng.choice(DE_NOTES)
        number = next(counter)
        cells = rng.randrange(180, 640)
        dividing = rng.randrange(9, 61)
        return (f'Präparat {number} ({rng.choice(DE_GROUPS)}): {rng.choice(DE_OBJECTS)}, fixiert in '
                f'{rng.choice(DE_FIXATIVES)}, gefärbt mit {rng.choice(DE_STAINS)}. Gezählt wurden '
                f'{cells} Zellen, davon {dividing} in Teilung. {rng.choice(DE_REMARKS)} '
                f'{rng.choice(DE_REMARKS)}')
    return Filler(make)


DE_POINTING = [
    ('sir-tubulin',
     'Für die Lebendfärbung der Spindelfasern mit SiR-Tubulin hat sich im Kurs ein eigenes '
     'Vorgehen bewährt, das von der Herstellerangabe abweicht. Konzentration, Einwirkzeit und '
     'Temperatur siehe nächster Abschnitt.',
     'Arbeitswerte zum zuvor genannten Farbstoff: 100 nanomolar im Kulturmedium, 90 Minuten '
     'einwirken lassen, bei 37 Grad Celsius; vor der Aufnahme nicht waschen.',
     'Wie lange und bei welcher Temperatur färbt man die Spindelfasern mit SiR-Tubulin?'),
    ('nocodazol',
     'Um Zellen in der Prometaphase zu sammeln, arretieren wir sie mit Nocodazol, das die '
     'Polymerisation der Mikrotubuli hemmt. Die Dosierung und den Zeitplan siehe nächster '
     'Abschnitt.',
     'Dosierung für den Arrest: 50 Nanogramm pro Milliliter über 16 Stunden; nach dem '
     'Auswaschen erreichen die Zellen nach etwa 45 Minuten die Anaphase.',
     'Welche Nocodazol-Konzentration nimmt man für den Arrest in der Prometaphase?'),
    ('kolchizin',
     'Für Chromosomenpräparate aus Zwiebelwurzelspitzen werden die Wurzeln vorher mit Kolchizin '
     'behandelt, damit sich mehr Zellen in der Metaphase ansammeln. Wie lange und in welcher '
     'Stärke, siehe nächster Abschnitt.',
     'Vorbehandlung: 0,05-prozentige Lösung, vier Stunden im Dunkeln; anschließend zehn Minuten '
     'in einmolarer Salzsäure bei 60 Grad hydrolysieren.',
     'Wie lange werden Zwiebelwurzelspitzen mit Kolchizin vorbehandelt?'),
    ('teilungsindex',
     'Für den Teilungsindex der Wurzelspitze gilt im Kurs eine verbindliche Zählregel, damit '
     'die Werte der Gruppen vergleichbar bleiben. Die Regel selbst siehe nächster Abschnitt.',
     'Zählregel: mindestens 1000 Zellen aus fünf Gesichtsfeldern; eine Prophase zählt erst, '
     'wenn die Chromosomen als Fäden sichtbar sind.',
     'Wie viele Zellen muss man für den Teilungsindex der Wurzelspitze mindestens auszählen?'),
    ('propidiumiodid',
     'Für die Messung des DNA-Gehalts im Durchflusszytometer werden die fixierten Zellen mit '
     'Propidiumiodid gefärbt, das in die Doppelhelix interkaliert. Ansatz und Einwirkzeit siehe '
     'nächster Abschnitt.',
     'Färbelösung: 20 Mikrogramm pro Milliliter mit RNase A; 30 Minuten bei Raumtemperatur und '
     'lichtgeschützt, dann ohne Waschen messen.',
     'In welcher Konzentration setzt man Propidiumiodid für die DNA-Gehaltsmessung ein?'),
    ('seeigel',
     'Die erste Furchungsteilung befruchteter Seeigeleier lässt sich im Kurs unter dem '
     'Mikroskop direkt verfolgen. Den Zeitplan nach der Befruchtung siehe nächster Abschnitt.',
     'Bei 18 Grad Celsius hebt sich die Befruchtungsmembran nach zwei Minuten ab; die erste '
     'Teilung folgt nach 85 Minuten, die zweite nach weiteren 40.',
     'Nach wie vielen Minuten teilen sich befruchtete Seeigeleier zum ersten Mal?'),
]

DE_CONTROLS = [
    ('karmin',
     'Karminessigsäure für Quetschpräparate wird zweiprozentig in 45-prozentiger Essigsäure '
     'angesetzt und vor Gebrauch filtriert.',
     'Wie wird Karminessigsäure für Quetschpräparate angesetzt?'),
    ('kalibrierung',
     'Die Mikroskope im Kursraum 2.14 kalibriert jeden Freitag Frau Dr. Henrike Albers mit dem '
     'Objektmikrometer.',
     'Wer kalibriert die Mikroskope im Kursraum?'),
    ('hela',
     'Eine HeLa-Zelle braucht in unserer Kultur rund 22 Stunden für die Interphase, die Mitose '
     'selbst dauert etwa 50 Minuten.',
     'Wie lange dauert die Mitose einer HeLa-Zelle in der Kurskultur?'),
    ('brutschrank',
     'Der Brutschrank B für die Zellkulturen läuft mit fünf Prozent Kohlendioxid und 95 Prozent '
     'Luftfeuchte.',
     'Mit wie viel Kohlendioxid läuft der Brutschrank für die Zellkulturen?'),
    ('ethidium',
     'Abfälle mit Ethidiumbromid gehören in den roten Sammelbehälter neben Abzug 3, nie in den '
     'Ausguss.',
     'Wohin gehören Abfälle mit Ethidiumbromid?'),
    ('auswertbar',
     'Ein Quetschpräparat der Wurzelspitze gilt als auswertbar, wenn mindestens drei Metaphasen '
     'im selben Gesichtsfeld liegen.',
     'Wann gilt ein Quetschpräparat der Wurzelspitze als auswertbar?'),
]

# ---------------------------------------------------------------- Spanish chronicle
ES_MONTHS = ['enero', 'febrero', 'marzo', 'abril', 'mayo', 'junio', 'julio', 'agosto',
             'septiembre', 'octubre', 'noviembre', 'diciembre']
ES_MEN = ['Agapito', 'Baldomero', 'Cipriano', 'Dionisio', 'Eladio', 'Fermín', 'Gumersindo',
          'Hilario', 'Isidoro', 'Leandro', 'Matías', 'Nicasio', 'Pascual', 'Remigio', 'Tomás']
ES_WOMEN = ['Agustina', 'Bernarda', 'Catalina', 'Dominga', 'Eulalia', 'Felisa', 'Gregoria',
            'Jacinta', 'Leocadia', 'Manuela', 'Petra', 'Rufina', 'Sinforosa', 'Tomasa', 'Vicenta']
ES_SURNAMES = ['Alonso', 'Barrientos', 'Cuesta', 'Del Río', 'Fidalgo', 'Gallego', 'Herrero',
               'Lobato', 'Mielgo', 'Nistal', 'Otero', 'Prieto', 'Rabanal', 'Santos', 'Turienzo']
ES_PLACES = ['la calle Real', 'la plaza del Moral', 'la calle de la Fuente', 'la Corredera',
             'la cuesta de San Roque', 'las casas del Ejido']
ES_WEATHER = [
    'Llovió casi toda la semana y los caminos de la vega quedaron intransitables.',
    'Heló varias noches seguidas y se perdió buena parte de la flor de los almendros.',
    'Hizo un calor impropio de la estación y los pozos bajaron de nivel.',
    'Sopló cierzo durante días y hubo que asegurar las tejas del pórtico.',
    'Cayó granizo en la parte alta del término, sin grandes daños.',
    'Nevó lo bastante para que los niños no acudieran a la escuela dos días.',
]


def spanish_filler(rng):
    """Dated entries in order; when December is past, the chronicle opens the next year."""
    state = {'year': 1911, 'month': 0, 'day': 1}

    def person(woman):
        name = rng.choice(ES_WOMEN if woman else ES_MEN)
        return f'{name} {rng.choice(ES_SURNAMES)} {rng.choice(ES_SURNAMES)}'

    def make():
        state['day'] += rng.randrange(2, 7)
        if state['day'] > 28:
            state['day'] = rng.randrange(1, 5)
            state['month'] += 1
            if state['month'] == 12:
                state.update(year=state['year'] + 1, month=0)
                return f"## {state['year']}"
        when = f"{state['day']} de {ES_MONTHS[state['month']]}"
        woman = rng.random() < 0.5
        kind = rng.random()
        if kind < 0.4:
            child = 'hija' if woman else 'hijo'
            return (f'{when}. Bautizo de {person(woman)}, {child} de {person(False)} y de '
                    f'{person(True)}, vecinos de {rng.choice(ES_PLACES)}; fueron padrinos '
                    f'{person(False)} y {person(True)}.')
        if kind < 0.6:
            return (f'{when}. Casamiento de {person(False)} con {person(True)}; fueron '
                    f'testigos {person(False)} y {person(False)}. {rng.choice(ES_WEATHER)}')
        if kind < 0.85:
            neighbour = 'vecina' if woman else 'vecino'
            rite = rng.choice(['primera', 'segunda', 'tercera'])
            return (f'{when}. Se dio sepultura a {person(woman)}, de {rng.randrange(2, 89)} '
                    f'años, {neighbour} de {rng.choice(ES_PLACES)}; recibió los sacramentos y se '
                    f'le hizo oficio de {rite} clase.')
        return f'{when}. {rng.choice(ES_WEATHER)} {rng.choice(ES_WEATHER)}'
    return Filler(make)


ES_POINTING = [
    ('campana',
     'El señor obispo aprobó por fin la refundición de la campana mayor, rajada desde la helada '
     'de enero, y la junta de fábrica cerró el trato con el fundidor, como se detalla a '
     'continuación.',
     'Ajuste de la obra: mil ochocientas cuarenta pesetas en dos plazos; la fundición corre a '
     'cargo del maestro Anselmo Garrido, de Palencia.',
     '¿Cuánto costó refundir la campana mayor y quién hizo la fundición?'),
    ('langosta',
     'La plaga de langosta entró en los trigos de la vega a primeros de mes; se hicieron '
     'rogativas y la cofradía socorrió a los labradores perjudicados, como se detalla a '
     'continuación.',
     'Daños y socorro: se perdió la tercera parte del trigo sembrado, y la cofradía repartió '
     'sesenta y cuatro fanegas de grano entre veintidós familias.',
     '¿Cuántas fanegas repartió la cofradía tras la plaga de langosta?'),
    ('patron',
     'La fiesta del patrón San Bartolomé se celebró este año con más solemnidad que nunca, con '
     'predicador forastero y música traída de fuera, como se detalla a continuación.',
     'Predicó el padre Lucio Escribano, franciscano de Zamora; tocó la banda de Benavente, y la '
     'limosna del día sumó trescientas doce pesetas.',
     '¿Quién predicó en la fiesta de San Bartolomé?'),
    ('sarampion',
     'Hubo un brote de sarampión entre los niños de la escuela, y el médico titular '
     'ordenó cerrar las aulas, como se detalla a continuación.',
     'Las aulas estuvieron cerradas del nueve al treinta de noviembre; enfermaron cuarenta y '
     'siete niños y fallecieron tres.',
     '¿Cuántos niños murieron en el brote de sarampión de la escuela?'),
    ('cementerio',
     'El ayuntamiento cedió terreno para ampliar el cementerio, que ya no daba abasto, y el '
     'arcipreste vino a bendecirlo, como se detalla a continuación.',
     'Se cedieron mil doscientos cincuenta metros cuadrados en el pago de Las Eras; los bendijo '
     'don Saturnino Pozo el catorce de abril.',
     '¿Cuántos metros cuadrados se cedieron para ampliar el cementerio?'),
    ('organo',
     'El órgano de la parroquia, mudo desde hacía años, volvió a sonar tras una reparación '
     'completa, como se detalla a continuación.',
     'Hizo el trabajo el organero Valentín Reguera, de Valladolid; duró veintitrés días y '
     'costó novecientas sesenta pesetas.',
     '¿Quién reparó el órgano de la parroquia y cuánto costó?'),
]

ES_CONTROLS = [
    ('luz',
     'Se inauguró la luz eléctrica en la iglesia, costeada por don Facundo Rebollo, indiano '
     'vuelto de Cuba.',
     '¿Quién costeó la luz eléctrica de la iglesia?'),
    ('parroco',
     'Tomó posesión de la parroquia el nuevo párroco, don Eusebio Carracedo, natural de Astorga.',
     '¿De dónde era natural el párroco Eusebio Carracedo?'),
    ('sequia',
     'Por la sequía se sacó en rogativa la imagen de la Virgen del Castillo hasta la ermita de '
     'los Remedios.',
     '¿Qué imagen se sacó en rogativa por la sequía?'),
    ('reloj',
     'Se estrenó el reloj de la torre, regalo de los emigrantes del pueblo en Buenos Aires.',
     '¿Quién regaló el reloj de la torre?'),
    ('animas',
     'La cofradía de las Ánimas eligió mayordomo a Teodoro Villafáñez en su junta ordinaria.',
     '¿A quién eligió mayordomo la cofradía de las Ánimas?'),
    ('escuela',
     'Llegó al pueblo la nueva maestra, doña Ramona Cifuentes, con plaza en propiedad para la '
     'escuela de niñas.',
     '¿Cómo se llamaba la nueva maestra de la escuela de niñas?'),
]


def fill(builder, pointing, controls, rng):
    builder.add_filler(rng.randrange(3, 6))
    controls = list(controls)
    for key, pointer, answer, _ in pointing:
        builder.pointing(key, pointer, answer)
        builder.add_filler(rng.randrange(8, 14))
        if controls:
            ckey, fact, _ = controls.pop(0)
            builder.add(fact, ckey)
            builder.add_filler(rng.randrange(8, 14))
    builder.add_filler(rng.randrange(4, 8))
    return builder


def main():
    tokenizer = Tokenizer.from_file(sys.argv[1])
    rng = random.Random(SEED)

    de = fill(Builder(tokenizer, '# Kurshandbuch Zellbiologie: Mitose und Zellzyklus',
                      german_filler(rng)), DE_POINTING, DE_CONTROLS, rng)

    es = fill(Builder(tokenizer, '# Crónica parroquial de San Bartolomé de Villanueva\n\n## 1911',
                      spanish_filler(rng)), ES_POINTING, ES_CONTROLS, rng)

    documents, queries = [], []
    for doc_id, title, builder, pointing, controls, language, subject in [
        ('kurshandbuch-zellbiologie', 'Kurshandbuch Zellbiologie: Mitose und Zellzyklus', de,
         DE_POINTING, DE_CONTROLS, 'de', 'German cell-biology handbook'),
        ('cronica-san-bartolome', 'Crónica parroquial de San Bartolomé de Villanueva', es,
         ES_POINTING, ES_CONTROLS, 'es', 'Spanish parish chronicle'),
    ]:
        text = builder.text()
        documents.append({'id': doc_id, 'title': title, 'language': language, 'text': text})
        raw = text.encode('utf-8')
        bounds = chunk_bounds(tokenizer, text)
        for key, pointer, answer, question in pointing:
            span = builder.spans[key]
            p_span = builder.spans[key + ':pointer']
            assert raw[span[0]:span[1]].decode() == answer
            p_in, a_in = touches(bounds, p_span), touches(bounds, span)
            assert len(p_in) == 1 and a_in == [p_in[0] + 1], (key, p_in, a_in)
            assert text.count(answer) == 1
            queries.append({
                'id': f'sc-{language}-next-{key}',
                'category': 'pointing clause',
                'dimensions': ['answer in the following passage', subject],
                'text': question,
                'relevance': {doc_id: 3},
                'answer_spans': {doc_id: [list(span)]},
            })
        for key, fact, question in controls:
            span = builder.spans[key]
            assert raw[span[0]:span[1]].decode() == fact
            assert holders(bounds, span), key
            assert text.count(fact) == 1
            queries.append({
                'id': f'sc-{language}-same-{key}',
                'category': 'plain fact',
                'dimensions': ['answer in the same passage', subject],
                'text': question,
                'relevance': {doc_id: 3},
                'answer_spans': {doc_id: [list(span)]},
            })
        print(f'{doc_id}: {len(raw)} bytes, {len(bounds)} passages', file=sys.stderr)

    dataset = {
        'name': 'lattice-section-context-v1',
        'description': (
            'Two long non-English, non-software documents, a German cell-biology course handbook '
            'and a Spanish parish chronicle, whose pointing needles name a topic in one passage '
            'and defer its values to the next ("siehe nächster Abschnitt", "como se detalla a '
            'continuación"); each document also states plain facts whole inside one passage, as '
            'controls. Passage placement was checked against the indexer\'s chunker (800 tokens, '
            '120 overlap, sentence boundaries) under the Qwen3-Embedding-0.6B tokenizer: a '
            'pointing clause lies in exactly one passage and its answer in exactly the next. '
            'Answerable queries carry `answer_spans`, the UTF-8 byte range of the answering '
            'paragraph. It measures whether section-neighbour context carried after the hits '
            'holds answers the hits alone miss: compare passage recall at top_k with twice '
            'top_k. See SECTION_CONTEXT_V1.md.'),
        'dimensions': ['answer in the following passage', 'answer in the same passage',
                       'German cell-biology handbook', 'Spanish parish chronicle'],
        'documents': documents,
        'queries': queries,
    }
    with open(sys.argv[2], 'w', encoding='utf-8') as out:
        json.dump(dataset, out, ensure_ascii=False, indent=2)
        out.write('\n')


if __name__ == '__main__':
    main()
