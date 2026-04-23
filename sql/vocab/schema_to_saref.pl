% Schema.org → SAREF (IoT Sensor Data) vocabulary alignment rules
% ─────────────────────────────────────────────────────────────────
%
% Maps Schema.org sensor/device properties to the Smart Appliances REFerence
% (SAREF) ontology for IoT use cases.
%
% Usage:
%   SELECT pg_ripple.load_vocab_template('schema_to_saref');
%   SELECT pg_ripple.infer('schema_to_saref');
%
% SAREF prefix: https://saref.etsi.org/core/
% Schema.org prefix: https://schema.org/

% --- Property alignments ---

% schema:name → saref:hasName
aligned_pred(?s, "https://saref.etsi.org/core/hasName", ?o) :-
    triple(?s, "https://schema.org/name", ?o).

% schema:description → saref:hasDescription
aligned_pred(?s, "https://saref.etsi.org/core/hasDescription", ?o) :-
    triple(?s, "https://schema.org/description", ?o).

% schema:measurementTechnique → saref:hasMeasurement
aligned_pred(?s, "https://saref.etsi.org/core/hasMeasurement", ?o) :-
    triple(?s, "https://schema.org/measurementTechnique", ?o).

% schema:unitCode → saref:isMeasuredIn
aligned_pred(?s, "https://saref.etsi.org/core/isMeasuredIn", ?o) :-
    triple(?s, "https://schema.org/unitCode", ?o).

% schema:value → saref:hasValue
aligned_pred(?s, "https://saref.etsi.org/core/hasValue", ?o) :-
    triple(?s, "https://schema.org/value", ?o).

% schema:Device → saref:Device
aligned_type(?s, "https://saref.etsi.org/core/Device") :-
    triple(?s, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
           "https://schema.org/Product").

% schema:PropertyValue → saref:Measurement
aligned_type(?s, "https://saref.etsi.org/core/Measurement") :-
    triple(?s, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
           "https://schema.org/PropertyValue").

% schema:additionalProperty → saref:hasProperty
aligned_pred(?s, "https://saref.etsi.org/core/hasProperty", ?o) :-
    triple(?s, "https://schema.org/additionalProperty", ?o).

% schema:location → saref:isLocatedIn
aligned_pred(?s, "https://saref.etsi.org/core/isLocatedIn", ?o) :-
    triple(?s, "https://schema.org/location", ?o).

% schema:serialNumber → saref:hasIdentifier
aligned_pred(?s, "https://saref.etsi.org/core/hasIdentifier", ?o) :-
    triple(?s, "https://schema.org/serialNumber", ?o).

% schema:dateCreated → saref:hasTimestamp
aligned_pred(?s, "https://saref.etsi.org/core/hasTimestamp", ?o) :-
    triple(?s, "https://schema.org/dateCreated", ?o).

% Emit aligned triples into the default graph
triple(?s, ?p, ?o) :-
    aligned_pred(?s, ?p, ?o).

triple(?s, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type", ?t) :-
    aligned_type(?s, ?t).
