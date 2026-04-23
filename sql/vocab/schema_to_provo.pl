# Schema.org → PROV-O (Provenance Ontology) vocabulary alignment rules
# ─────────────────────────────────────────────────────────────────────
#
# Maps Schema.org properties to W3C PROV-O provenance terms.
#
# Usage:
#   SELECT pg_ripple.load_vocab_template('schema_to_provo');
#   SELECT pg_ripple.infer('schema_to_provo');
#
# PROV-O prefix: http://www.w3.org/ns/prov#
# Schema.org prefix: https://schema.org/

# --- Type alignments ---

# schema:Person → prov:Person
aligned_type(?s, "http://www.w3.org/ns/prov#Person") :-
    triple(?s, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
           "https://schema.org/Person").

# schema:Organization → prov:Organization
aligned_type(?s, "http://www.w3.org/ns/prov#Organization") :-
    triple(?s, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
           "https://schema.org/Organization").

# schema:Action → prov:Activity
aligned_type(?s, "http://www.w3.org/ns/prov#Activity") :-
    triple(?s, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
           "https://schema.org/Action").

# schema:CreativeWork → prov:Entity
aligned_type(?s, "http://www.w3.org/ns/prov#Entity") :-
    triple(?s, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
           "https://schema.org/CreativeWork").

# --- Property alignments ---

# schema:agent → prov:wasAssociatedWith
aligned_pred(?s, "http://www.w3.org/ns/prov#wasAssociatedWith", ?o) :-
    triple(?s, "https://schema.org/agent", ?o).

# schema:startTime → prov:startedAtTime
aligned_pred(?s, "http://www.w3.org/ns/prov#startedAtTime", ?o) :-
    triple(?s, "https://schema.org/startTime", ?o).

# schema:endTime → prov:endedAtTime
aligned_pred(?s, "http://www.w3.org/ns/prov#endedAtTime", ?o) :-
    triple(?s, "https://schema.org/endTime", ?o).

# schema:result → prov:generated
aligned_pred(?s, "http://www.w3.org/ns/prov#generated", ?o) :-
    triple(?s, "https://schema.org/result", ?o).

# schema:object → prov:used
aligned_pred(?s, "http://www.w3.org/ns/prov#used", ?o) :-
    triple(?s, "https://schema.org/object", ?o).

# schema:creator → prov:wasAttributedTo
aligned_pred(?s, "http://www.w3.org/ns/prov#wasAttributedTo", ?o) :-
    triple(?s, "https://schema.org/creator", ?o).

# schema:dateCreated → prov:generatedAtTime
aligned_pred(?s, "http://www.w3.org/ns/prov#generatedAtTime", ?o) :-
    triple(?s, "https://schema.org/dateCreated", ?o).

# schema:dateModified → prov:invalidatedAtTime
aligned_pred(?s, "http://www.w3.org/ns/prov#invalidatedAtTime", ?o) :-
    triple(?s, "https://schema.org/dateModified", ?o).

# schema:isBasedOn → prov:wasDerivedFrom
aligned_pred(?s, "http://www.w3.org/ns/prov#wasDerivedFrom", ?o) :-
    triple(?s, "https://schema.org/isBasedOn", ?o).

# schema:sameAs → prov:alternateOf
aligned_pred(?s, "http://www.w3.org/ns/prov#alternateOf", ?o) :-
    triple(?s, "https://schema.org/sameAs", ?o).

# Emit aligned triples into the default graph
triple(?s, ?p, ?o) :-
    aligned_pred(?s, ?p, ?o).

triple(?s, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type", ?t) :-
    aligned_type(?s, ?t).
