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

# --- Patient resource (schema:Person → fhir:Patient) ---

?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://hl7.org/fhir/Patient> :-
    ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://schema.org/Person> .

# schema:givenName → fhir:Patient.name.given
?s <https://hl7.org/fhir/Patient.name.given> ?o :-
    ?s <https://schema.org/givenName> ?o .

# schema:familyName → fhir:Patient.name.family
?s <https://hl7.org/fhir/Patient.name.family> ?o :-
    ?s <https://schema.org/familyName> ?o .

# schema:birthDate → fhir:Patient.birthDate
?s <https://hl7.org/fhir/Patient.birthDate> ?o :-
    ?s <https://schema.org/birthDate> ?o .

# schema:gender → fhir:Patient.gender
?s <https://hl7.org/fhir/Patient.gender> ?o :-
    ?s <https://schema.org/gender> ?o .

# schema:email → fhir:Patient.telecom
?s <https://hl7.org/fhir/Patient.telecom> ?o :-
    ?s <https://schema.org/email> ?o .

# --- Observation resource (schema:MedicalObservation → fhir:Observation) ---

?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://hl7.org/fhir/Observation> :-
    ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://schema.org/MedicalObservation> .

# schema:name → fhir:Observation.code.text (for MedicalObservation)
?s <https://hl7.org/fhir/Observation.code.text> ?o :-
    ?s <https://schema.org/name> ?o,
    ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://schema.org/MedicalObservation> .

# schema:value → fhir:Observation.valueString (for MedicalObservation)
?s <https://hl7.org/fhir/Observation.valueString> ?o :-
    ?s <https://schema.org/value> ?o,
    ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://schema.org/MedicalObservation> .

# schema:dateCreated → fhir:Observation.effectiveDateTime (for MedicalObservation)
?s <https://hl7.org/fhir/Observation.effectiveDateTime> ?o :-
    ?s <https://schema.org/dateCreated> ?o,
    ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://schema.org/MedicalObservation> .

# --- Practitioner resource (schema:Physician → fhir:Practitioner) ---

?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://hl7.org/fhir/Practitioner> :-
    ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://schema.org/Physician> .
