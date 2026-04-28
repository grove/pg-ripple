[← Back to Blog Index](README.md)

# The Four Built-in Rule Sets: What RDFS, OWL RL, OWL EL, and OWL QL Actually Do

## A practical guide to every inference rule shipped with pg_ripple — and when to use each one

---

You loaded 50,000 triples into pg_ripple. Your ontology says `Dog rdfs:subClassOf Mammal` and `Mammal rdfs:subClassOf Animal`. A user queries for all Animals. They get zero results, because nobody explicitly typed any dog as an Animal.

The data is there. The logical implication is obvious to a human. But your database doesn't reason — it stores and retrieves. If a fact wasn't explicitly inserted, it doesn't exist.

pg_ripple ships four built-in rule sets that close this gap. Each one is a collection of Datalog rules compiled to SQL and executed inside PostgreSQL. Load a rule set, run inference, and the derived facts appear as real triples — queryable by SPARQL, indexed, and indistinguishable from explicit data (except for a `source = 1` flag that marks them as inferred).

This post walks through every rule in all four sets: what it does, why it exists, and when it fires.

---

## The Basics: How Built-in Rules Work

All four rule sets live in [`src/datalog/builtins.rs`](../src/datalog/builtins.rs). Each is a `const &str` block of Datalog rules that pg_ripple compiles to SQL at load time. The workflow is identical for all:

```sql
-- Step 1: Load the rules into the engine
SELECT pg_ripple.load_rules_builtin('rdfs');  -- or 'owl-rl', 'owl-el', 'owl-ql'

-- Step 2: Run inference (semi-naive evaluation)
SELECT pg_ripple.infer('rdfs');

-- Step 3: Query — inferred triples are now visible
SELECT * FROM pg_ripple.sparql('
  SELECT ?x ?type WHERE { ?x rdf:type ?type }
');
```

Rules are executed bottom-up. Recursive rules iterate until no new facts are derived (fixpoint). Derived triples are stored in VP delta tables with `source = 1`. You can layer rule sets — run RDFS first, then OWL RL on top — because each stratum builds on previously derived facts.

Now let's look at what each set actually does.

---

## 1. RDFS Rules — The Foundation (13 Rules)

**Load with:** `load_rules_builtin('rdfs')`

RDFS (RDF Schema) is the base entailment regime for the Semantic Web. It defines how class hierarchies, property hierarchies, and type membership work. If you're using `rdfs:subClassOf`, `rdfs:subPropertyOf`, `rdfs:domain`, or `rdfs:range` anywhere in your data, you need RDFS inference to make them actually mean something.

### rdfs2 — Domain Inference

```
?x rdf:type ?c :- ?x ?p ?y, ?p rdfs:domain ?c .
```

**What it does:** If property `p` has domain `c`, then any resource that appears as the *subject* of `p` is an instance of `c`.

**Example:** `foaf:name` has domain `foaf:Person`. You insert `ex:alice foaf:name "Alice"`. RDFS infers `ex:alice rdf:type foaf:Person` — Alice is a Person, because she has a name, and names belong to people.

**Why it matters:** Domain declarations let you assert typing constraints once in your ontology instead of manually tagging every entity. This rule makes those declarations operational.

### rdfs3 — Range Inference

```
?y rdf:type ?c :- ?x ?p ?y, ?p rdfs:range ?c .
```

**What it does:** If property `p` has range `c`, then any resource that appears as the *object* of `p` is an instance of `c`.

**Example:** `ex:worksAt` has range `schema:Organization`. You insert `ex:alice ex:worksAt ex:mit`. RDFS infers `ex:mit rdf:type schema:Organization`.

**Why it matters:** The range counterpart of rdfs2. Together they let properties imply types in both directions.

### rdfs4a, rdfs4b — Everything Is a Resource

```
?x rdf:type rdfs:Resource :- ?x ?p ?y .
?y rdf:type rdfs:Resource :- ?x ?p ?y .
```

**What they do:** Every subject and object mentioned in any triple is an `rdfs:Resource`.

**In practice:** These rules produce a huge number of trivially true triples. pg_ripple's inference engine can optionally eliminate them via subsumption checking (they show up in the `eliminated_rules` field of `infer_with_stats`).

### rdfs5 — SubPropertyOf Transitivity

```
?p rdfs:subPropertyOf ?r :- ?p rdfs:subPropertyOf ?q, ?q rdfs:subPropertyOf ?r .
```

**What it does:** If `p` is a sub-property of `q` and `q` is a sub-property of `r`, then `p` is a sub-property of `r`.

**Example:** `ex:isPartOf rdfs:subPropertyOf ex:relatedTo`. `ex:isComponentOf rdfs:subPropertyOf ex:isPartOf`. This rule derives `ex:isComponentOf rdfs:subPropertyOf ex:relatedTo`.

### rdfs6 — SubPropertyOf Reflexivity

```
?p rdfs:subPropertyOf ?p :- ?p rdf:type rdf:Property .
```

**What it does:** Every property is a sub-property of itself. A technical axiom that ensures reflexivity.

### rdfs7 — SubPropertyOf Propagation

```
?x ?r ?y :- ?x ?p ?y, ?p rdfs:subPropertyOf ?r .
```

**What it does:** If `p` is a sub-property of `r`, then every `p`-triple also holds for `r`.

**Example:** `dct:creator rdfs:subPropertyOf dct:contributor`. You insert `ex:paper dct:creator ex:alice`. RDFS infers `ex:paper dct:contributor ex:alice`.

**Why it matters:** This is the workhorse rule for property hierarchies. It lets you query at a general level (`dct:contributor`) and automatically pick up results from more specific predicates (`dct:creator`).

### rdfs8 — Class Identity

```
?x rdf:type rdfs:Class :- ?x rdf:type rdfs:Class .
```

**What it does:** A class is an instance of `rdfs:Class`. This is a tautology that seeds the type system.

### rdfs9 — SubClassOf Type Propagation

```
?x rdf:type ?c :- ?x rdf:type ?b, ?b rdfs:subClassOf ?c .
```

**What it does:** If `x` is of type `b` and `b` is a subclass of `c`, then `x` is of type `c`.

**Example:** `ex:rex rdf:type ex:Dog`. `ex:Dog rdfs:subClassOf ex:Mammal`. RDFS infers `ex:rex rdf:type ex:Mammal`.

**Why it matters:** This is the rule that makes class hierarchies actually useful. Without it, a query for all Mammals doesn't return Dogs, even though the ontology says every Dog is a Mammal.

### rdfs10 — SubClassOf Reflexivity

```
?c rdfs:subClassOf ?c :- ?c rdf:type rdfs:Class .
```

**What it does:** Every class is a subclass of itself.

### rdfs11 — SubClassOf Transitivity

```
?b rdfs:subClassOf ?c :- ?b rdfs:subClassOf ?a, ?a rdfs:subClassOf ?c .
```

**What it does:** Subclass relationships are transitive. If Dog ⊑ Mammal ⊑ Animal, then Dog ⊑ Animal.

**Why it matters:** Combined with rdfs9, this gives you full class hierarchy traversal. You declare the direct subclass links; RDFS computes the transitive closure.

### rdfs12 — Container Membership Properties

```
?p rdfs:subPropertyOf rdfs:member :- ?p rdf:type rdfs:ContainerMembershipProperty .
```

**What it does:** The RDF container membership properties (`rdf:_1`, `rdf:_2`, etc.) are all sub-properties of `rdfs:member`.

### rdfs13 — Datatype Hierarchy

```
rdfs:Datatype rdfs:subClassOf rdfs:Literal :- rdfs:Datatype rdf:type rdfs:Class .
```

**What it does:** All datatypes are literals. A schema-level axiom.

### When to Use RDFS

**Always.** RDFS is the base layer. If your data uses any RDF vocabulary — `rdfs:subClassOf`, `rdfs:domain`, `rdfs:range`, `rdfs:subPropertyOf` — then RDFS inference makes those declarations produce actual triples. Without it, class hierarchies and property hierarchies are decorative metadata that nothing acts on.

RDFS is also fast. 13 rules, mostly one-step joins. On a 10 million triple graph, full RDFS inference typically completes in under a second.

---

## 2. OWL RL Rules — The General-Purpose Workhorse (~80 Rules)

**Load with:** `load_rules_builtin('owl-rl')`

OWL RL (Rule Language) is the OWL 2 profile designed for rule-based systems. It extends RDFS with symmetric properties, transitive properties, inverse properties, class constructors, property chains, identity reasoning, and more. It's the profile most users want.

The OWL RL rule set in pg_ripple includes all RDFS rules as stratum 0, then adds ~70 OWL-specific rules on top. Here's what they cover.

### Included: All RDFS Rules

The OWL RL set starts by including the complete RDFS rule set. This means loading `owl-rl` alone gives you everything RDFS provides, plus the OWL extensions.

### Symmetric Properties

```
?y ?p ?x :- ?x ?p ?y, ?p rdf:type owl:SymmetricProperty .
```

**What it does:** If `p` is symmetric and `x p y`, then `y p x`.

**Example:** `foaf:knows` is symmetric. You insert `ex:alice foaf:knows ex:bob`. OWL RL infers `ex:bob foaf:knows ex:alice`.

**Why it matters:** Relationships that are inherently bidirectional — friendship, adjacency, co-authorship — shouldn't require you to insert both directions manually.

### Transitive Properties

```
?x ?p ?z :- ?x ?p ?y, ?y ?p ?z, ?p rdf:type owl:TransitiveProperty .
```

**What it does:** If `p` is transitive and `x p y` and `y p z`, then `x p z`.

**Example:** `ex:locatedIn` is transitive. Cambridge is located in Massachusetts. Massachusetts is located in the USA. OWL RL infers Cambridge is located in the USA.

**Why it matters:** Geographic containment, organizational hierarchies, part-of relationships — any relationship that chains naturally. You declare the direct links; the engine computes the reachable set.

### Inverse Properties

```
?y ?q ?x :- ?x ?p ?y, ?p owl:inverseOf ?q .
?y ?p ?x :- ?x ?q ?y, ?p owl:inverseOf ?q .
```

**What it does:** If `p` is the inverse of `q`, then every `p`-triple generates a `q`-triple in the opposite direction, and vice versa.

**Example:** `ex:cites owl:inverseOf ex:citedBy`. You insert `ex:paper1 ex:cites ex:paper2`. OWL RL infers `ex:paper2 ex:citedBy ex:paper1`.

**Why it matters:** Citation networks, parent/child, employs/employedBy — you model one direction and get the other for free.

### Functional and Inverse-Functional Properties

```
?y1 owl:sameAs ?y2 :- ?x ?p ?y1, ?x ?p ?y2, ?p rdf:type owl:FunctionalProperty .
?x1 owl:sameAs ?x2 :- ?x1 ?p ?y, ?x2 ?p ?y, ?p rdf:type owl:InverseFunctionalProperty .
```

**What they do:**
- A **functional** property can have at most one value per subject. If the data has two values, they must be the same entity → `owl:sameAs`.
- An **inverse-functional** property uniquely identifies its subject. If two subjects share a value, they must be the same entity → `owl:sameAs`.

**Example:** `foaf:mbox` is inverse-functional (each email identifies one person). If `ex:alice foaf:mbox <mailto:alice@mit.edu>` and `ex:dr_smith foaf:mbox <mailto:alice@mit.edu>`, then OWL RL infers `ex:alice owl:sameAs ex:dr_smith`.

**Why it matters:** This is the foundation of entity resolution in OWL. Inverse-functional properties like email addresses, SSNs, or DOIs become automatic deduplication keys.

### owl:sameAs Reasoning

```
?y owl:sameAs ?x :- ?x owl:sameAs ?y .
?x owl:sameAs ?z :- ?x owl:sameAs ?y, ?y owl:sameAs ?z .
?y rdf:type ?c :- ?x rdf:type ?c, ?x owl:sameAs ?y .
```

**What they do:** `owl:sameAs` is symmetric and transitive. If two entities are the same, they share all type memberships.

**Why it matters:** Once the functional/inverse-functional rules establish `sameAs` links, these rules propagate all properties across the equivalence class. pg_ripple further optimizes this with union-find canonicalization in the dictionary, so query-time expansion is avoided entirely.

### Equivalent Classes and Properties

```
?x rdf:type ?c2 :- ?x rdf:type ?c1, ?c1 owl:equivalentClass ?c2 .
?x ?p2 ?y :- ?x ?p1 ?y, ?p1 owl:equivalentProperty ?p2 .
```

**What they do:** Equivalent classes share all instances. Equivalent properties share all triples.

**Example:** `schema:Person owl:equivalentClass foaf:Person`. If `ex:alice rdf:type schema:Person`, OWL RL infers `ex:alice rdf:type foaf:Person`. You can query with either vocabulary and get the same results.

**Why it matters:** When integrating data from multiple sources that use different ontologies, equivalent-class and equivalent-property mappings are the standard way to bridge vocabularies without rewriting data.

### Property Chain Axioms

```
?x ?p ?z :- ?x ?p1 ?y, ?y ?p2 ?z, ?p owl:propertyChainAxiom ?chain .
?x ?p ?w :- ?x ?p1 ?y, ?y ?p2 ?z, ?z ?p3 ?w, ?p owl:propertyChainAxiom ?chain .
```

**What they do:** Property chains compose two or three properties into a derived property.

**Example:** Define `ex:uncleOf owl:propertyChainAxiom (ex:parentOf ex:brotherOf)`. If Alice is the parent of Bob and Bob is the brother of Charlie, OWL RL infers `ex:alice ex:uncleOf ex:charlie`. (Three-hop chains work the same way for longer compositions.)

**Why it matters:** Family relationships, organizational reporting lines, geographic containment ("country of birthplace" = birthplace → locatedIn → country) — complex relationships expressed as compositions of simple ones.

### Class Constructors

```
# allValuesFrom
?y rdf:type ?c :- ?x rdf:type ?r, ?x ?p ?y, ?r owl:allValuesFrom ?c, ?r owl:onProperty ?p .

# hasValue
?x rdf:type ?r :- ?x ?p ?v, ?r owl:hasValue ?v, ?r owl:onProperty ?p .

# intersectionOf
?x rdf:type ?c :- ?x rdf:type ?c1, ?x rdf:type ?c2, ?c owl:intersectionOf ?list .
```

**What they do:**
- **allValuesFrom:** If `x` is of type `R` and `R` restricts property `p` to `allValuesFrom C`, then every value of `x` via `p` is of type `C`.
- **hasValue:** If `x` has value `v` via property `p`, and restriction `R` says `onProperty p, hasValue v`, then `x` is of type `R`.
- **intersectionOf:** If `x` is an instance of every class in the intersection, then `x` is an instance of the intersection class.

**Example:** A restriction `VeganFood owl:allValuesFrom VegetableIngredient` on property `containsIngredient`. If a food item is typed as `VeganFood`, then everything it contains is inferred to be a `VegetableIngredient`.

### Bidirectional SubClassOf → EquivalentClass

```
?c1 owl:equivalentClass ?c2 :- ?c1 rdfs:subClassOf ?c2, ?c2 rdfs:subClassOf ?c1 .
```

**What it does:** If two classes are mutual subclasses, they are equivalent.

**Why it matters:** Ontology normalization. Two independently defined classes that happen to have the same extension are formally linked.

### Inconsistency Detection

```
?s rdf:type owl:Nothing :- ?s owl:sameAs ?o, ?s owl:differentFrom ?o .
```

**What it does:** If an entity is stated to be both `sameAs` and `differentFrom` another entity, that's a contradiction. The entity is classified as `owl:Nothing`.

**Why it matters:** This catches logical errors in your data — someone claimed two entities are identical and different at the same time. Instances of `owl:Nothing` are a signal to investigate data quality.

### Numeric Datatype Hierarchy

```
?lt rdf:type xsd:decimal :- ?lt rdf:type xsd:integer .
?lt rdf:type xsd:numeric :- ?lt rdf:type xsd:decimal .
?lt rdf:type xsd:integer :- ?lt rdf:type xsd:nonNegativeInteger .
?lt rdf:type xsd:integer :- ?lt rdf:type xsd:long .
?lt rdf:type xsd:long    :- ?lt rdf:type xsd:int .
?lt rdf:type xsd:int     :- ?lt rdf:type xsd:short .
?lt rdf:type xsd:short   :- ?lt rdf:type xsd:byte .
...
```

**What they do:** XSD numeric type promotion. An `xsd:integer` is also an `xsd:decimal`, which is also an `xsd:numeric`. A `xsd:byte` is a `xsd:short`, which is an `xsd:int`, and so on.

**Why it matters:** SPARQL FILTER expressions that compare numeric types rely on the promotion hierarchy. Without these rules, a filter like `FILTER(?age > 18)` might miss integer-typed values when comparing against a decimal.

### When to Use OWL RL

OWL RL is the default recommendation. Use it when:

- Your ontology uses OWL constructs beyond RDFS (inverse, symmetric, transitive properties, class restrictions, equivalence).
- You're integrating data from multiple sources with different vocabularies.
- You need entity resolution via `owl:sameAs` and inverse-functional properties.
- You want the "RDFS on steroids" experience without thinking about which profile to pick.

Performance: ~80 rules. On a 10 million triple graph with a typical T-Box, full materialization takes seconds with parallel stratum evaluation. For targeted queries, combine with magic sets to avoid materializing everything.

---

## 3. OWL EL Rules — Large Terminologies (Classification-Focused)

**Load with:** `load_rules_builtin('owl-el')`

OWL EL was designed for one specific scenario: ontologies with **very large class hierarchies** — hundreds of thousands to millions of classes — where you need to compute the complete subsumption hierarchy in polynomial time. The canonical examples are SNOMED CT (350,000+ concepts), Gene Ontology (45,000+ terms), and ChEBI (170,000+ chemical entities).

Where OWL RL is general-purpose, OWL EL is specialized. It trades away features like symmetric properties and disjointness for a guarantee: **classification is always polynomial in the size of the ontology**, regardless of structure or depth.

### SubClassOf Transitivity

```
?c rdfs:subClassOf ?e :- ?c rdfs:subClassOf ?d, ?d rdfs:subClassOf ?e .
```

**What it does:** The same as rdfs11 — transitive closure of the subclass hierarchy. Included here so OWL EL is self-contained (you don't need to load RDFS separately).

### Class Intersection — Decomposition

```
?x rdf:type ?c1 :- ?x rdf:type ?c, ?c owl:intersectionOf ?c1 .
?x rdf:type ?c2 :- ?x rdf:type ?c, ?c owl:intersectionOf ?c2 .
```

**What it does:** If `x` is an instance of an intersection class, then `x` is an instance of each conjunct.

**Example:** `InfectiousBacterialDisease owl:intersectionOf (InfectiousDisease, BacterialDisease)`. If a patient has `InfectiousBacterialDisease`, they also have `InfectiousDisease` and `BacterialDisease` individually.

**Why it matters:** Medical ontologies define diseases, drugs, and procedures as intersections of multiple dimensions. This rule decomposes those definitions for querying.

### Class Intersection — Composition

```
?x rdf:type ?c :- ?x rdf:type ?c1, ?x rdf:type ?c2, ?c owl:intersectionOf ?c1, ?c owl:intersectionOf ?c2 .
```

**What it does:** The reverse — if `x` is an instance of every conjunct, then `x` is an instance of the intersection class.

**Example:** If a condition is both an `InfectiousDisease` and a `BacterialDisease`, and the ontology defines `InfectiousBacterialDisease` as their intersection, the condition is classified as `InfectiousBacterialDisease`.

### Existential Restriction — someValuesFrom (Forward)

```
?y rdf:type ?b :- ?x rdf:type ?r, ?r owl:someValuesFrom ?b, ?r owl:onProperty ?p, ?x ?p ?y .
```

**What it does:** If `x` is of a type that has an existential restriction ("some values from `B` via property `p`"), and `x` actually has a value `y` via `p`, then `y` is of type `B`.

**Example:** The restriction `HeartDisease owl:someValuesFrom Heart` on property `affectsSite`. If a condition is typed as `HeartDisease` and it `affectsSite` some organ, that organ is inferred to be a `Heart`.

**Why it matters:** This is the signature OWL EL feature. Existential restrictions are pervasive in medical ontologies — "has finding site", "has causative agent", "has method" — and this rule makes them produce type inferences.

### Existential Restriction — someValuesFrom (Reverse)

```
?x rdf:type ?r :- ?x ?p ?y, ?y rdf:type ?b, ?r owl:someValuesFrom ?b, ?r owl:onProperty ?p .
```

**What it does:** The reverse direction — if `x` has a value `y` via `p`, and `y` is of type `B`, and there's a restriction class `R` defined as "some values from `B` via `p`", then `x` is of type `R`.

**Example:** If a condition `affectsSite` an organ that is of type `Heart`, and there exists a restriction class `HeartDisease` defined as `someValuesFrom Heart` on `affectsSite`, then the condition is classified as a `HeartDisease`.

**Why it matters:** This is the classification rule that OWL EL is built around. Given a set of properties and their value types, it automatically classifies entities into the most specific restriction classes in the ontology. This is what tools like ELK and SNOMED CT classifiers do — and pg_ripple does it inside PostgreSQL.

### Universal Restriction — allValuesFrom

```
?y rdf:type ?b :- ?x rdf:type ?r, ?r owl:allValuesFrom ?b, ?r owl:onProperty ?p, ?x ?p ?y .
```

**What it does:** If `x` is of type `R` and `R` restricts property `p` to `allValuesFrom B`, then every value of `x` via `p` is of type `B`.

### EquivalentClass — Bidirectional Subsumption

```
?c rdfs:subClassOf ?d :- ?c owl:equivalentClass ?d .
?d rdfs:subClassOf ?c :- ?c owl:equivalentClass ?d .
```

**What it does:** Equivalent classes imply mutual subclass relationships. This feeds the transitive closure computation.

### Class Membership from SubClassOf

```
?x rdf:type ?d :- ?x rdf:type ?c, ?c rdfs:subClassOf ?d .
```

**What it does:** Same as rdfs9 — type propagation through the class hierarchy. Included for self-containment.

### SubPropertyOf Propagation

```
?x ?q ?y :- ?x ?p ?y, ?p rdfs:subPropertyOf ?q .
```

**What it does:** Same as rdfs7 — property hierarchy propagation.

### Class Union

```
?x rdf:type ?c :- ?x rdf:type ?c1, ?c owl:unionOf ?c1 .
?x rdf:type ?c :- ?x rdf:type ?c2, ?c owl:unionOf ?c2 .
```

**What it does:** If `x` is an instance of any member of a union, then `x` is an instance of the union class.

**Example:** `LivingOrganism owl:unionOf (Animal, Plant, Microorganism)`. If something is an `Animal`, it's a `LivingOrganism`.

### When to Use OWL EL

Use OWL EL when:

- You have a **large terminological ontology** (tens of thousands to millions of classes).
- Your ontology uses **existential restrictions** (`owl:someValuesFrom`) to define classes.
- You need the **complete subsumption hierarchy** computed and materialized.
- **Polynomial-time guarantees** matter — you can't tolerate worst-case exponential blowup.
- You're working with **SNOMED CT, ICD-11, Gene Ontology, ChEBI, NCI Thesaurus**, or similar biomedical ontologies.

Do NOT use OWL EL when:

- You need symmetric properties (use OWL RL).
- You need transitive properties beyond subClassOf/subPropertyOf (use OWL RL).
- You need `owl:sameAs` reasoning (use OWL RL).
- You need disjointness checking (use OWL QL or OWL RL).

---

## 4. OWL QL Rules — Query Rewriting Without Materialization

**Load with:** `load_rules_builtin('owl-ql')`

OWL QL (Query Language) is fundamentally different from the other three profiles. Where RDFS, OWL RL, and OWL EL **materialize** inferred triples — computing them in advance and storing them — OWL QL **rewrites queries** so that inferences are computed on the fly during query execution.

The advantage: no materialization step, no storage overhead, and ontology changes take effect immediately without re-inference. The trade-off: query rewriting only works for a limited set of axiom types, and complex queries may be slower than pre-materialized lookups.

pg_ripple's `owl-ql` built-in ships a Datalog-expressible subset of OWL 2 QL axioms. For full query rewriting (the QL speciality), the engine in `src/sparql/ql_rewrite.rs` handles SPARQL-level rewriting directly. The Datalog rules here cover the materializable fragment for cases where you want to pre-compute QL inferences.

### SubClassOf Type Propagation

```
?x rdf:type ?b :- ?x rdf:type ?a, ?a rdfs:subClassOf ?b .
```

**What it does:** Same as rdfs9. In QL mode, this rule is used for the materializable fragment; the rewriter handles it at query time in pure QL mode.

### Existential in Superclass Position

```
?x rdf:type ?a :- ?x ?r ?y, ?c owl:someValuesFrom owl:Thing, ?c owl:onProperty ?r, ?c rdfs:subClassOf ?a .
```

**What it does:** If there exists a restriction class `C` defined as "some values from `owl:Thing` via property `r`", and `C` is a subclass of `A`, then anything that has a value via `r` is of type `A`.

**Example:** The ontology says "anything that has a `parentOf` relationship is a `Parent`" (formalized as `someValuesFrom(parentOf, owl:Thing) ⊑ Parent`). If `ex:alice ex:parentOf ex:bob`, QL infers `ex:alice rdf:type ex:Parent`.

**Why it matters:** This is QL's signature pattern — domain-like inference expressed as an existential restriction. It's more expressive than `rdfs:domain` because it can be refined with subclass chains.

### SubPropertyOf Propagation

```
?x ?q ?y :- ?x ?p ?y, ?p rdfs:subPropertyOf ?q .
```

**What it does:** Standard property hierarchy propagation.

### Inverse Properties

```
?y ?p ?x :- ?x ?q ?y, ?p owl:inverseOf ?q .
?y ?q ?x :- ?x ?p ?y, ?p owl:inverseOf ?q .
```

**What they do:** Bidirectional inverse property inference, same as OWL RL.

### Disjointness Checking

```
?x rdf:type owl:Nothing :- ?x rdf:type ?c1, ?x rdf:type ?c2, ?c1 owl:disjointWith ?c2 .
```

**What it does:** If an individual is an instance of two classes that are declared disjoint, classify it as `owl:Nothing` — a logical contradiction.

**Example:** `Male owl:disjointWith Female`. If `ex:pat` is typed as both `Male` and `Female`, QL flags `ex:pat` as `owl:Nothing`.

**Why it matters:** Disjointness is the only negation-like construct in QL. It catches data quality errors where an entity violates exclusivity constraints.

### EquivalentClass — Bidirectional

```
?c rdfs:subClassOf ?d :- ?c owl:equivalentClass ?d .
?d rdfs:subClassOf ?c :- ?c owl:equivalentClass ?d .
```

**What it does:** Same as OWL EL — mutual subsumption from equivalence.

### Functional Property → sameAs

```
?y1 owl:sameAs ?y2 :- ?x ?p ?y1, ?x ?p ?y2, ?p rdf:type owl:FunctionalProperty .
```

**What it does:** Same as OWL RL — functional properties with multiple values imply identity.

### sameAs Symmetry and Type Propagation

```
?y owl:sameAs ?x :- ?x owl:sameAs ?y .
?x rdf:type ?c :- ?x owl:sameAs ?y, ?y rdf:type ?c .
```

**What they do:** `owl:sameAs` is symmetric, and identical entities share types.

### The Query Rewriting Mode

The real power of OWL QL isn't in these Datalog rules — it's in the query rewriter. When you set `pg_ripple.owl_profile = 'ql'`, SPARQL queries are automatically expanded at translation time:

```sql
SET pg_ripple.owl_profile = 'ql';

-- Query: "find all Mammals"
SELECT * FROM pg_ripple.sparql('
  SELECT ?x WHERE { ?x rdf:type ex:Mammal }
');
```

If the ontology says `Dog rdfs:subClassOf Mammal` and `Cat rdfs:subClassOf Mammal`, the rewriter expands this to:

```sql
-- Rewritten: find instances of Mammal OR Dog OR Cat
SELECT s FROM vp_rdf_type WHERE o IN (encode('Mammal'), encode('Dog'), encode('Cat'))
```

No inferred triples were stored. The ontology axioms were applied at query compile time. If you later add `Hamster rdfs:subClassOf Mammal`, the next query automatically picks it up without re-running inference.

### When to Use OWL QL

Use OWL QL when:

- Your ontology changes frequently and re-materialization is expensive.
- Your data store is small relative to the potential materialization (few triples, many axioms).
- You need **instant ontology updates** without waiting for inference.
- Your axioms are limited to subClassOf, subPropertyOf, inverseOf, someValuesFrom, disjointWith.
- You're building an ontology-mediated query answering system.

Do NOT use OWL QL when:

- Your queries are latency-sensitive and materialized lookups would be faster.
- You need symmetric or transitive property reasoning (use OWL RL).
- You need existential restrictions beyond the superclass position (use OWL EL).
- Your data is large and queries are frequent — materialization amortizes the cost.

---

## Comparison: All Four Profiles at a Glance

| Feature | RDFS | OWL RL | OWL EL | OWL QL |
|---|---|---|---|---|
| **Rules** | 13 | ~80 | ~15 | ~12 |
| **Approach** | Materialize | Materialize | Materialize | Rewrite (+ partial materialize) |
| **SubClassOf transitivity** | ✅ | ✅ | ✅ | ✅ |
| **SubPropertyOf propagation** | ✅ | ✅ | ✅ | ✅ |
| **Domain/range inference** | ✅ | ✅ | — | — |
| **Symmetric properties** | — | ✅ | — | — |
| **Transitive properties** | — | ✅ | — | — |
| **Inverse properties** | — | ✅ | — | ✅ |
| **owl:sameAs** | — | ✅ | — | ✅ (limited) |
| **Functional properties** | — | ✅ | — | ✅ |
| **equivalentClass** | — | ✅ | ✅ | ✅ |
| **intersectionOf** | — | ✅ | ✅ | — |
| **unionOf** | — | — | ✅ | — |
| **someValuesFrom** | — | ✅ (limited) | ✅ (full) | ✅ (superclass only) |
| **allValuesFrom** | — | ✅ | ✅ | — |
| **hasValue** | — | ✅ | — | — |
| **Property chains** | — | ✅ | — | — |
| **Disjointness** | — | ✅ | — | ✅ |
| **Datatype hierarchy** | — | ✅ | — | — |
| **Best for** | Baseline | General enterprise | SNOMED / biomedical | OMQA / volatile ontologies |
| **Tractability** | Linear | Polynomial | Polynomial (guaranteed) | Sub-polynomial |

---

## Layering: Using Multiple Profiles Together

The four rule sets are not mutually exclusive. You can layer them:

```sql
-- Layer 1: RDFS baseline
SELECT pg_ripple.load_rules_builtin('rdfs');
SELECT pg_ripple.infer('rdfs');

-- Layer 2: OWL RL on top of RDFS
SELECT pg_ripple.load_rules_builtin('owl-rl');
SELECT pg_ripple.infer('owl-rl');

-- Layer 3: Custom domain rules on top of everything
SELECT pg_ripple.load_rules('
  ?paper ex:relatedTo ?other :-
      ?paper dct:subject ?topic ,
      ?other dct:subject ?topic .
', 'domain');
SELECT pg_ripple.infer('domain');
```

Each stratum builds on the previous one's derived facts. Run them in order — base entailment first, then progressively richer profiles.

You can also combine EL for classification with RL for property reasoning:

```sql
-- Classify the ontology first (polynomial-time hierarchy)
SELECT pg_ripple.load_rules_builtin('owl-el');
SELECT pg_ripple.infer('owl-el');

-- Then apply RL for property-level reasoning on top
SELECT pg_ripple.load_rules_builtin('owl-rl');
SELECT pg_ripple.infer('owl-rl');
```

Or use QL for query-time expansion on dimensions where materialization isn't worth it, while keeping RL-materialized facts for the hot paths.

---

## Choosing Your Profile

**"I don't know what I need."** → Start with RDFS. Add OWL RL when you hit a construct RDFS doesn't cover.

**"I need symmetric/transitive/inverse properties."** → OWL RL.

**"I have SNOMED CT / Gene Ontology / a huge taxonomy."** → OWL EL.

**"My ontology changes hourly and I can't re-materialize."** → OWL QL.

**"I need everything."** → Load OWL RL (which includes RDFS). Add OWL EL if you have large terminological hierarchies. Enable QL rewriting for the volatile dimensions.

The beauty of pg_ripple's approach is that all four profiles are just Datalog rule sets compiled to SQL. They're not four separate engines — they're four different programs running on the same engine. Mix, match, and layer as your data demands.
