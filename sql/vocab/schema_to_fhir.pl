# Schema.org → FHIR R4 (Healthcare) vocabulary alignment rules
# ─────────────────────────────────────────────────────────────────
#
# Maps Schema.org person/medical properties to FHIR R4 basic resources
# (Patient, Observation, Practitioner).
#
# Usage:
#   SELECT pg_ripple.load_vocab_template('schema_to_fhir');
#   SELECT pg_ripple.infer('schema_to_fhir');
#
# FHIR prefix: https://hl7.org/fhir/
# Schema.org prefix: https://schema.org/

# --- Patient resource alignments ---

# schema:Person → fhir:Patient
aligned_type(?s, "https://hl7.org/fhir/Patient") :-
    triple(?s, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
           "https://schema.org/Person").

# schema:givenName → fhir:Patient.name.given
aligned_pred(?s, "https://hl7.org/fhir/Patient.name.given", ?o) :-
    triple(?s, "https://schema.org/givenName", ?o).

# schema:familyName → fhir:Patient.name.family
aligned_pred(?s, "https://hl7.org/fhir/Patient.name.family", ?o) :-
    triple(?s, "https://schema.org/familyName", ?o).

# schema:birthDate → fhir:Patient.birthDate
aligned_pred(?s, "https://hl7.org/fhir/Patient.birthDate", ?o) :-
    triple(?s, "https://schema.org/birthDate", ?o).

# schema:gender → fhir:Patient.gender
aligned_pred(?s, "https://hl7.org/fhir/Patient.gender", ?o) :-
    triple(?s, "https://schema.org/gender", ?o).

# schema:email → fhir:Patient.telecom (email)
aligned_pred(?s, "https://hl7.org/fhir/Patient.telecom", ?o) :-
    triple(?s, "https://schema.org/email", ?o).

# --- Observation resource alignments ---

# schema:MedicalObservation → fhir:Observation
aligned_type(?s, "https://hl7.org/fhir/Observation") :-
    triple(?s, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
           "https://schema.org/MedicalObservation").

# schema:name → fhir:Observation.code.text
aligned_pred(?s, "https://hl7.org/fhir/Observation.code.text", ?o) :-
    triple(?s, "https://schema.org/name", ?o),
    triple(?s, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
           "https://schema.org/MedicalObservation").

# schema:value → fhir:Observation.valueString
aligned_pred(?s, "https://hl7.org/fhir/Observation.valueString", ?o) :-
    triple(?s, "https://schema.org/value", ?o),
    triple(?s, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
           "https://schema.org/MedicalObservation").

# schema:dateCreated → fhir:Observation.effectiveDateTime
aligned_pred(?s, "https://hl7.org/fhir/Observation.effectiveDateTime", ?o) :-
    triple(?s, "https://schema.org/dateCreated", ?o),
    triple(?s, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
           "https://schema.org/MedicalObservation").

# schema:Practitioner → fhir:Practitioner
aligned_type(?s, "https://hl7.org/fhir/Practitioner") :-
    triple(?s, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
           "https://schema.org/Physician").

# Emit aligned triples into the default graph
triple(?s, ?p, ?o) :-
    aligned_pred(?s, ?p, ?o).

triple(?s, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type", ?t) :-
    aligned_type(?s, ?t).
