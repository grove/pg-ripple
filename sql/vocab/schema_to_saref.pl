# Schema.org → SAREF (IoT Sensor Data) vocabulary alignment rules
# ─────────────────────────────────────────────────────────────────
#
# Maps Schema.org sensor/device properties to the Smart Appliances REFerence
# (SAREF) ontology for IoT use cases.
#
# Usage:
#   SELECT pg_ripple.load_vocab_template('schema_to_saref');
#   SELECT pg_ripple.infer('schema_to_saref');
#
# SAREF prefix: https://saref.etsi.org/core/
# Schema.org prefix: https://schema.org/

# --- Property alignments ---

# schema:name → saref:hasName
?s <https://saref.etsi.org/core/hasName> ?o :-
    ?s <https://schema.org/name> ?o .

# schema:description → saref:hasDescription
?s <https://saref.etsi.org/core/hasDescription> ?o :-
    ?s <https://schema.org/description> ?o .

# schema:measurementTechnique → saref:hasMeasurement
?s <https://saref.etsi.org/core/hasMeasurement> ?o :-
    ?s <https://schema.org/measurementTechnique> ?o .

# schema:unitCode → saref:isMeasuredIn
?s <https://saref.etsi.org/core/isMeasuredIn> ?o :-
    ?s <https://schema.org/unitCode> ?o .

# schema:value → saref:hasValue
?s <https://saref.etsi.org/core/hasValue> ?o :-
    ?s <https://schema.org/value> ?o .

# schema:additionalProperty → saref:hasProperty
?s <https://saref.etsi.org/core/hasProperty> ?o :-
    ?s <https://schema.org/additionalProperty> ?o .

# schema:location → saref:isLocatedIn
?s <https://saref.etsi.org/core/isLocatedIn> ?o :-
    ?s <https://schema.org/location> ?o .

# schema:serialNumber → saref:hasIdentifier
?s <https://saref.etsi.org/core/hasIdentifier> ?o :-
    ?s <https://schema.org/serialNumber> ?o .

# schema:dateCreated → saref:hasTimestamp
?s <https://saref.etsi.org/core/hasTimestamp> ?o :-
    ?s <https://schema.org/dateCreated> ?o .

# schema:Product → saref:Device (type alignment)
?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://saref.etsi.org/core/Device> :-
    ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://schema.org/Product> .

# schema:PropertyValue → saref:Measurement (type alignment)
?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://saref.etsi.org/core/Measurement> :-
    ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://schema.org/PropertyValue> .

