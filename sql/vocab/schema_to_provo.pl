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
?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/ns/prov#Person> :-
    ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://schema.org/Person> .

# schema:Organization → prov:Organization
?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/ns/prov#Organization> :-
    ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://schema.org/Organization> .

# schema:Action → prov:Activity
?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/ns/prov#Activity> :-
    ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://schema.org/Action> .

# schema:CreativeWork → prov:Entity
?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/ns/prov#Entity> :-
    ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://schema.org/CreativeWork> .

# --- Property alignments ---

# schema:agent → prov:wasAssociatedWith
?s <http://www.w3.org/ns/prov#wasAssociatedWith> ?o :-
    ?s <https://schema.org/agent> ?o .

# schema:startTime → prov:startedAtTime
?s <http://www.w3.org/ns/prov#startedAtTime> ?o :-
    ?s <https://schema.org/startTime> ?o .

# schema:endTime → prov:endedAtTime
?s <http://www.w3.org/ns/prov#endedAtTime> ?o :-
    ?s <https://schema.org/endTime> ?o .

# schema:result → prov:generated
?s <http://www.w3.org/ns/prov#generated> ?o :-
    ?s <https://schema.org/result> ?o .

# schema:object → prov:used
?s <http://www.w3.org/ns/prov#used> ?o :-
    ?s <https://schema.org/object> ?o .

# schema:creator → prov:wasAttributedTo
?s <http://www.w3.org/ns/prov#wasAttributedTo> ?o :-
    ?s <https://schema.org/creator> ?o .

# schema:dateCreated → prov:generatedAtTime
?s <http://www.w3.org/ns/prov#generatedAtTime> ?o :-
    ?s <https://schema.org/dateCreated> ?o .

# schema:dateModified → prov:invalidatedAtTime
?s <http://www.w3.org/ns/prov#invalidatedAtTime> ?o :-
    ?s <https://schema.org/dateModified> ?o .

# schema:isBasedOn → prov:wasDerivedFrom
?s <http://www.w3.org/ns/prov#wasDerivedFrom> ?o :-
    ?s <https://schema.org/isBasedOn> ?o .

# schema:sameAs → prov:alternateOf
?s <http://www.w3.org/ns/prov#alternateOf> ?o :-
    ?s <https://schema.org/sameAs> ?o .
