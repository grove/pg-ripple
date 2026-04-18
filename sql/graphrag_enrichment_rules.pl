# GraphRAG Datalog Enrichment Rules for pg_ripple (v0.26.0)
#
# Derives implicit relationships that LLM extraction misses.
# Load into pg_ripple:
#   SELECT pg_ripple.load_rules(
#       pg_read_file('/path/to/graphrag_enrichment_rules.pl'),
#       'graphrag_enrichment'
#   );
# Run inference:
#   SELECT pg_ripple.infer('graphrag_enrichment');
#
# Prerequisites:
#   - OWL-RL built-ins for RDFS transitivity:
#       SELECT pg_ripple.load_rules_builtin('owl-rl');
#       SELECT pg_ripple.infer('owl-rl');
#   - Base triples from graphrag_ontology.ttl must be loaded.
#
# Rule syntax: Turtle-flavoured Datalog.
#   head_s head_p head_o :- body_atom_1, body_atom_2, ... .
#   Variables: ?name
#   Constants: <https://full-iri> or prefixed (if registered)

# ── gr:coworker ───────────────────────────────────────────────────────────────
# Two entities are coworkers if they both appear as the source of separate
# relationships that target the same organization entity.
# Use case: enriches Local Search by surfacing indirect colleague connections
# the LLM missed because it only extracted direct mentions together.
#
# ?a and ?b are coworkers when:
#   ?rel1 points from ?a to ?org AND ?rel2 points from ?b to ?org
#   (and ?rel1 != ?rel2, guaranteed by requiring distinct source bindings)
?a <https://graphrag.org/ns/coworker> ?b :- ?rel1 <https://graphrag.org/ns/source> ?a, ?rel2 <https://graphrag.org/ns/source> ?b, ?rel1 <https://graphrag.org/ns/target> ?org, ?rel2 <https://graphrag.org/ns/target> ?org .

# ── gr:collaborates ───────────────────────────────────────────────────────────
# Two entities collaborate if they are both mentioned in the same text unit.
# Use case: enriches DRIFT search by finding entities that co-occur in source
# text even without an explicit named relationship.
#
# ?a and ?b collaborate when:
#   text unit ?tu mentions both ?a and ?b
?a <https://graphrag.org/ns/collaborates> ?b :- ?tu <https://graphrag.org/ns/mentionsEntity> ?a, ?tu <https://graphrag.org/ns/mentionsEntity> ?b .

# ── gr:indirectReport ─────────────────────────────────────────────────────────
# Transitive management chain: ?leader indirectly manages ?sub2 when there
# is a chain of gr:manages relationships from ?leader down to ?sub2.
# Use case: enriches Global Search by making org hierarchy fully navigable,
# even when the LLM only extracted direct manager-report relationships.
#
# Base case: direct management is already an indirect report
?leader <https://graphrag.org/ns/indirectReport> ?sub :- ?leader <https://graphrag.org/ns/manages> ?sub .

# Recursive case: transitive closure
?leader <https://graphrag.org/ns/indirectReport> ?sub2 :- ?leader <https://graphrag.org/ns/indirectReport> ?mid, ?mid <https://graphrag.org/ns/manages> ?sub2 .

# ── gr:relatedOrg ─────────────────────────────────────────────────────────────
# Two organizations are related if they share at least one entity that
# participates as a source in a relationship targeting both organizations.
# (Co-occurrence threshold approximation using shared relationship endpoints.)
# Use case: enriches community detection by adding cross-organizational edges
# that the LLM extracted individually but did not group.
#
# ?orgA and ?orgB are related when:
#   some entity ?e has a relationship to ?orgA AND a relationship to ?orgB
?orgA <https://graphrag.org/ns/relatedOrg> ?orgB :- ?rel1 <https://graphrag.org/ns/source> ?e, ?rel1 <https://graphrag.org/ns/target> ?orgA, ?rel2 <https://graphrag.org/ns/source> ?e, ?rel2 <https://graphrag.org/ns/target> ?orgB .
