# pg_ripple × OpenClaw: Knowledge-Powered Personal AI

> **Date**: 2026-04-18
> **Status**: Research report
> **Audience**: pg_ripple developers and OpenClaw community builders

---

## Executive Summary

**OpenClaw** is the dominant open-source personal AI assistant runtime (360k GitHub stars, 1,679 contributors). It connects to 25+ messaging channels, runs LLM-powered agent loops with tool access, and maintains persistent memory via Markdown files indexed into SQLite with hybrid vector + BM25 search.

**pg_ripple** is a PostgreSQL 18 extension implementing a high-performance RDF triple store with full SPARQL 1.1, SHACL validation, Datalog reasoning, HTAP storage, federation, JSON-LD framing, and a companion HTTP/SPARQL endpoint.

These systems occupy **different layers of the AI stack** and have no functional overlap. OpenClaw is the agent runtime (how users interact with AI); pg_ripple is the knowledge backend (how AI stores, reasons over, and retrieves structured data). This report maps out concrete integration paths where pg_ripple upgrades OpenClaw from a **stateless chat agent** into a **knowledge-driven reasoning system** — and where OpenClaw gives pg_ripple a natural consumer-facing distribution channel.

---

## 1. OpenClaw Architecture (Relevant Subsystems)

### 1.1 Memory System

OpenClaw's memory is file-based:

| Layer | Storage | Loaded when |
|---|---|---|
| `MEMORY.md` | Long-term durable facts | Every DM session start |
| `memory/YYYY-MM-DD.md` | Daily notes | Today + yesterday auto-loaded |
| `DREAMS.md` | Dreaming consolidation summaries | Human review surface |
| Session transcripts | JSONL per session | On-demand via compaction |

**Search**: The built-in memory engine indexes Markdown chunks (~400 tokens, 80-token overlap) into a per-agent SQLite database. Search runs two parallel pipelines: vector similarity (via OpenAI, Gemini, Voyage, Mistral, Ollama, or local GGUF embeddings) and BM25 keyword matching, merged by weighted fusion.

**Dreaming**: An optional background consolidation system with light/deep/REM phases that scores short-term memory signals and promotes qualified candidates into `MEMORY.md` based on frequency, relevance, query diversity, recency, consolidation, and conceptual richness scores.

**Memory Wiki**: A companion plugin (`memory-wiki`) that compiles durable memory into a structured wiki vault with deterministic pages, structured claims (with `id`, `text`, `status`, `confidence`, `evidence[]`), contradiction tracking, dashboards, and machine-readable digests (`agent-digest.json`, `claims.jsonl`).

### 1.2 Plugin System

OpenClaw plugins are TypeScript ESM modules registered via `definePluginEntry` with a rich `register(api)` callback. Key registration methods:

| Method | What it does |
|---|---|
| `api.registerTool(tool)` | Agent tool (LLM-callable function) |
| `api.registerService(service)` | Background service (long-running) |
| `api.registerHook(events, handler)` | Lifecycle hooks (before/after tool calls, compaction, etc.) |
| `api.registerHttpRoute(params)` | Gateway HTTP endpoint |
| `api.registerMemoryCapability(cap)` | Exclusive memory backend |
| `api.registerMemoryCorpusSupplement(adapter)` | Additive memory search corpus |
| `api.registerMemoryPromptSupplement(builder)` | Additive memory prompt section |
| `api.registerCommand(def)` | Custom slash command |
| `api.registerCli(registrar)` | CLI subcommand |

Plugins can also expose **skills** (SKILL.md files) alongside their tools.

### 1.3 Skill System

Skills are markdown files with YAML frontmatter that teach the agent *when and how* to use tools. They load from workspace `skills/` directories and can be gated by environment, config, and binary requirements. Skills are the **primary lightweight extension mechanism** — no TypeScript required.

### 1.4 Automation

- **Cron**: Built-in scheduler with one-shot and recurring jobs, isolated or main-session execution, channel delivery
- **Webhooks**: HTTP endpoints (`/hooks/wake`, `/hooks/agent`, mapped hooks) that trigger agent turns
- **Hooks**: Plugin lifecycle hooks for `before_tool_call`, `after_tool_call`, `message_received`, `session_start`, `agent_end`, `before_compaction`, etc.

### 1.5 Agent Loop

```
Message → Session Resolution → Context Assembly → System Prompt Build
  → Model Inference → Tool Execution → Streaming Reply → Persistence
```

Key extension points for pg_ripple integration:
1. **`before_prompt_build`**: Inject SPARQL-retrieved context into the system prompt
2. **`after_tool_call`**: Extract entities/relationships from tool results and store as RDF
3. **`before_compaction`**: Save structured facts to the knowledge graph before context is summarized
4. **Agent tools**: Register SPARQL query/update tools available to the LLM

---

## 2. Gap Analysis: Where OpenClaw Falls Short

| Limitation | Impact | pg_ripple Solution |
|---|---|---|
| **Flat-file memory** | No structured queries over facts; `MEMORY.md` grows linearly and becomes noisy | RDF triples with SPARQL 1.1 query engine |
| **No relational reasoning** | Cannot derive "Alice knows Bob, Bob knows Carol → Alice is 2 hops from Carol" | Datalog reasoning + property path queries |
| **No schema enforcement** | Extracted facts can be contradictory, incomplete, or malformed | SHACL shape validation on assertion |
| **Search is document-level** | Memory search returns text chunks, not structured facts with provenance | SPARQL queries return precise fact tuples |
| **No cross-entity queries** | Cannot answer "Who are all the people I've discussed who work in ML?" without scanning all memory | VP-table indexed joins across entity types |
| **Dreaming is LLM-scored** | Promotion decisions depend on embedding quality and frequency heuristics | Datalog rules can enforce domain-specific promotion criteria |
| **Memory Wiki claims lack grounding** | Claims are text with optional evidence, not graph-connected facts | RDF-star statement-level provenance |
| **No temporal reasoning** | Cannot answer "What changed since last week?" without date parsing in text | Named graphs per time period + SPARQL temporal queries |
| **Compaction loses structure** | When context is summarized, precise facts are reduced to prose | Facts stored as triples survive compaction; only the conversation is compressed |

---

## 3. Integration Architecture

### 3.1 Two-Layer Integration Model

The integration operates at two levels:

1. **Plugin layer** (`@openclaw/pg-ripple`): A native OpenClaw plugin that registers tools, hooks, a background service, and memory corpus supplements. Written in TypeScript, communicates with pg_ripple via `pg_ripple_http` REST/SPARQL endpoints.

2. **Skill layer** (`skills/knowledge-graph/SKILL.md`): A skill that teaches the agent *when* to use the knowledge graph tools — without requiring the plugin to be installed (works with just `curl` + pg_ripple_http for simpler setups).

```
┌─────────────────────────────────────────────────────────────┐
│  OpenClaw Gateway                                           │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │ Plugin: @openclaw/pg-ripple                           │  │
│  │                                                       │  │
│  │  Tools:                                               │  │
│  │    kg_query     — SPARQL SELECT/CONSTRUCT             │  │
│  │    kg_store     — Assert triples from conversation    │  │
│  │    kg_facts     — Retrieve all facts about an entity  │  │
│  │    kg_similar   — Hybrid SPARQL + vector search       │  │
│  │    kg_infer     — Trigger Datalog reasoning           │  │
│  │    kg_validate  — Run SHACL validation                │  │
│  │                                                       │  │
│  │  Hooks:                                               │  │
│  │    before_prompt_build  → inject relevant KG context  │  │
│  │    before_compaction    → extract + store facts        │  │
│  │    after_tool_call      → auto-extract entities       │  │
│  │                                                       │  │
│  │  Services:                                            │  │
│  │    KG sync worker (embedding + inference refresh)     │  │
│  │                                                       │  │
│  │  Memory corpus supplement:                            │  │
│  │    memory_search corpus=kg → SPARQL-backed recall     │  │
│  └──────────────────────────┬────────────────────────────┘  │
│                             │                               │
│                     HTTP/SPARQL Protocol                     │
│                             │                               │
└─────────────────────────────┼───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  pg_ripple_http  (SPARQL endpoint, port 7878)               │
│                                                             │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  pg_ripple (PostgreSQL 18 Extension)                   │ │
│  │                                                        │ │
│  │  VP Tables ─── Dictionary ─── HTAP ─── SPARQL 1.1     │ │
│  │  Datalog ──── SHACL ──── pgvector ──── Federation      │ │
│  │  JSON-LD Framing ──── Named Graphs ──── FTS            │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 Communication Protocol

The plugin communicates with pg_ripple via `pg_ripple_http`:

| Endpoint | Plugin use |
|---|---|
| `POST /sparql` (SELECT) | `kg_query`, `kg_facts`, context retrieval |
| `POST /sparql` (CONSTRUCT) | JSON-LD framing for LLM context windows |
| `POST /sparql` (UPDATE) | `kg_store`, entity extraction |
| `GET /health` | Service health check |
| `GET /metrics` | Background monitoring |

The bearer token auth (`PG_RIPPLE_HTTP_AUTH_TOKEN`) secures the connection. For local-only setups (OpenClaw + pg_ripple on the same machine), the endpoint can bind to `127.0.0.1`.

---

## 4. Plugin Design: `@openclaw/pg-ripple`

### 4.1 Agent Tools

#### `kg_query` — Execute SPARQL Queries

```typescript
api.registerTool({
  name: "kg_query",
  description: "Query the knowledge graph using SPARQL. Returns structured results.",
  parameters: Type.Object({
    query: Type.String({ description: "SPARQL SELECT or CONSTRUCT query" }),
    format: Type.Optional(Type.Enum({ json: "json", table: "table", jsonld: "jsonld" })),
  }),
  async execute(_id, params) {
    const response = await fetch(`${PG_RIPPLE_URL}/sparql`, {
      method: "POST",
      headers: {
        "Content-Type": "application/sparql-query",
        "Accept": params.format === "jsonld"
          ? "application/ld+json"
          : "application/sparql-results+json",
        "Authorization": `Bearer ${AUTH_TOKEN}`,
      },
      body: params.query,
    });
    const data = await response.json();
    return { content: [{ type: "text", text: formatResults(data) }] };
  },
});
```

**When the agent should use this**: When the user asks a question that requires querying stored knowledge — "Who did I meet at the conference?", "What are the dependencies of project X?", "Show me all tasks assigned to Sarah."

#### `kg_store` — Assert Facts

```typescript
api.registerTool({
  name: "kg_store",
  description: "Store a fact in the knowledge graph as an RDF triple.",
  parameters: Type.Object({
    subject: Type.String({ description: "The entity (e.g., 'Sarah', 'ProjectX')" }),
    predicate: Type.String({ description: "The relationship (e.g., 'worksOn', 'deadline')" }),
    object: Type.String({ description: "The value (e.g., 'machine learning', '2026-06-01')" }),
    graph: Type.Optional(Type.String({ description: "Named graph for grouping" })),
  }),
  async execute(_id, params) {
    const sparqlUpdate = buildInsertData(params);
    await fetch(`${PG_RIPPLE_URL}/sparql`, {
      method: "POST",
      headers: {
        "Content-Type": "application/sparql-update",
        "Authorization": `Bearer ${AUTH_TOKEN}`,
      },
      body: sparqlUpdate,
    });
    return { content: [{ type: "text", text: `Stored: ${params.subject} → ${params.predicate} → ${params.object}` }] };
  },
});
```

**When the agent should use this**: When the user explicitly says "remember that...", "note that...", or when a conversation contains a fact worth persisting (project deadlines, preferences, relationships between people).

#### `kg_facts` — Entity Profile

```typescript
api.registerTool({
  name: "kg_facts",
  description: "Retrieve all known facts about a specific entity.",
  parameters: Type.Object({
    entity: Type.String({ description: "Entity name or IRI to look up" }),
    depth: Type.Optional(Type.Number({ description: "How many hops of related entities (default: 1)" })),
  }),
  // Generates: SELECT ?p ?o WHERE { <entity> ?p ?o }
  // With depth > 1: adds property path traversal
});
```

#### `kg_similar` — Hybrid Semantic + Structural Search

```typescript
api.registerTool({
  name: "kg_similar",
  description: "Find entities semantically similar to a query, filtered by graph constraints.",
  parameters: Type.Object({
    query: Type.String({ description: "Natural language description" }),
    type: Type.Optional(Type.String({ description: "RDF type filter (e.g., 'Person', 'Project')" })),
    limit: Type.Optional(Type.Number({ default: 10 })),
  }),
  // Generates SPARQL with pg:similar() function (v0.27.0+)
  // Falls back to FTS search on pre-v0.27.0 deployments
});
```

#### `kg_infer` — Trigger Reasoning

```typescript
api.registerTool({
  name: "kg_infer",
  description: "Run Datalog reasoning rules to derive new facts from existing knowledge.",
  parameters: Type.Object({
    ruleset: Type.String({ description: "Rule set name (e.g., 'owl-rl', 'domain_rules')" }),
  }),
  // Calls: SELECT pg_ripple.infer('<ruleset>')
});
```

#### `kg_validate` — Check Knowledge Quality

```typescript
api.registerTool({
  name: "kg_validate",
  description: "Validate the knowledge graph against SHACL shapes. Returns violations.",
  parameters: Type.Object({
    shape: Type.Optional(Type.String({ description: "Specific shape to validate (default: all)" })),
  }),
  // Calls: SELECT pg_ripple.shacl_validate(...)
});
```

### 4.2 Lifecycle Hooks

#### `before_prompt_build` — Contextual Knowledge Injection

Before each agent turn, the plugin queries pg_ripple for facts relevant to the current conversation and injects them into the system prompt:

```typescript
api.on("before_prompt_build", async (event) => {
  const { messages } = event;
  const lastUserMessage = getLastUserMessage(messages);
  if (!lastUserMessage) return {};

  // 1. Extract entity mentions from the user message
  const entities = extractEntityMentions(lastUserMessage);

  // 2. Query pg_ripple for relevant facts
  const sparql = buildContextQuery(entities, lastUserMessage);
  const facts = await queryPgRipple(sparql);

  // 3. If hybrid search is available (v0.27.0+), also do semantic retrieval
  const similar = await semanticSearch(lastUserMessage, 5);

  // 4. Format as compact context block
  const contextBlock = formatKnowledgeContext(facts, similar);

  return {
    prependContext: contextBlock
      ? `## Knowledge Graph Context\n${contextBlock}`
      : undefined,
  };
});
```

This gives the agent **automatic structured recall** without the agent needing to explicitly call `kg_query`.

#### `before_compaction` — Fact Extraction Before Context Loss

When compaction is about to summarize older conversation turns, the plugin extracts structured facts and stores them as triples:

```typescript
api.on("before_compaction", async (event) => {
  const { messages } = event;

  // Extract entities and relationships from conversation being compacted
  // This could use the LLM itself or a lightweight NER model
  const extracted = await extractFactsFromConversation(messages);

  // Store as RDF triples with provenance
  for (const fact of extracted) {
    await storeFact(fact, {
      graph: `urn:openclaw:session:${event.sessionId}`,
      confidence: fact.confidence,
      extractedAt: new Date().toISOString(),
    });
  }
});
```

#### `after_tool_call` — Automatic Knowledge Extraction

When the agent calls tools that produce structured data (web searches, file reads, API calls), the plugin can extract and store relevant entities:

```typescript
api.on("after_tool_call", async (event) => {
  const { toolName, result } = event;

  // Only extract from information-rich tools
  if (!EXTRACTION_TOOLS.includes(toolName)) return;

  const entities = await extractEntitiesFromToolResult(result);
  if (entities.length > 0) {
    await batchStoreEntities(entities, {
      graph: `urn:openclaw:tool:${toolName}`,
      source: "auto-extraction",
    });
  }
});
```

### 4.3 Memory Corpus Supplement

The plugin registers itself as an additional memory search corpus, so `memory_search corpus=all` or `memory_search corpus=kg` queries the knowledge graph alongside regular memory:

```typescript
api.registerMemoryCorpusSupplement({
  id: "kg",
  name: "Knowledge Graph",
  async search(query, options) {
    // Translate natural language query to SPARQL
    const sparql = buildSearchQuery(query, options.limit);
    const results = await queryPgRipple(sparql);

    return results.map((r) => ({
      content: formatFactAsText(r),
      source: `kg:${r.subject}`,
      score: r.score ?? 0.5,
      metadata: { type: "knowledge-graph", entity: r.subject },
    }));
  },
  async get(id) {
    // Resolve kg:<entity> to full fact set
    const entity = id.replace("kg:", "");
    const facts = await getEntityFacts(entity);
    return { content: formatFactsAsText(facts), source: id };
  },
});
```

This means OpenClaw's existing `memory_search` tool transparently queries the knowledge graph — the agent doesn't need to know about pg_ripple to benefit from it.

### 4.4 Background Service

A background service handles periodic tasks:

```typescript
api.registerService({
  id: "kg-sync",
  name: "Knowledge Graph Sync",
  async start(context) {
    // 1. Periodic inference refresh (run Datalog rules on new facts)
    context.schedule("0 */4 * * *", async () => {
      await triggerInference("owl-rl");
    });

    // 2. Periodic SHACL validation (flag quality issues)
    context.schedule("0 6 * * *", async () => {
      const violations = await runValidation();
      if (violations.length > 0) {
        context.notify(`Knowledge graph has ${violations.length} quality issues`);
      }
    });

    // 3. Embedding refresh for new entities (v0.27.0+)
    context.schedule("*/30 * * * *", async () => {
      await refreshEmbeddings();
    });
  },
});
```

---

## 5. Skill Design: `knowledge-graph`

For deployments where the full plugin is too heavy, a standalone skill teaches the agent to use pg_ripple_http directly:

```markdown
---
name: knowledge-graph
description: Query and manage a personal knowledge graph powered by pg_ripple.
metadata:
  {
    "openclaw": {
      "requires": {
        "env": ["PG_RIPPLE_HTTP_URL"]
      },
      "primaryEnv": "PG_RIPPLE_HTTP_URL"
    }
  }
---

# Knowledge Graph Skill

You have access to a personal knowledge graph stored in pg_ripple,
a PostgreSQL-based RDF triple store with SPARQL 1.1 support.

## When to use

- User asks you to **remember** a fact, preference, or relationship
- User asks a **structural question** ("Who works on project X?",
  "What are all the tasks due this week?")
- User asks about **connections** between people, projects, or topics
- Before answering questions where you suspect relevant context exists
  in past conversations

## How to query

Use the `exec` tool to run curl against the SPARQL endpoint:

```bash
curl -s -X POST "$PG_RIPPLE_HTTP_URL/sparql" \
  -H "Content-Type: application/sparql-query" \
  -H "Accept: application/sparql-results+json" \
  -H "Authorization: Bearer $PG_RIPPLE_HTTP_AUTH_TOKEN" \
  -d 'SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10'
```

## How to store facts

Use SPARQL UPDATE to insert triples:

```bash
curl -s -X POST "$PG_RIPPLE_HTTP_URL/sparql" \
  -H "Content-Type: application/sparql-update" \
  -H "Authorization: Bearer $PG_RIPPLE_HTTP_AUTH_TOKEN" \
  -d 'PREFIX ex: <http://example.org/>
      INSERT DATA {
        ex:Sarah ex:worksOn "machine learning" .
        ex:Sarah a ex:Person .
      }'
```

## Ontology conventions

Use these prefixes consistently:
- `ex:` for user entities (people, projects, tasks)
- `rdf:type` for classification
- `rdfs:label` for display names
- `xsd:date` for date literals
- Named graphs for temporal grouping: `<urn:openclaw:YYYY-MM-DD>`

## Examples

### "Remember that Sarah works on ML"
```sparql
INSERT DATA {
  GRAPH <urn:openclaw:2026-04-18> {
    ex:Sarah a ex:Person ;
      rdfs:label "Sarah" ;
      ex:worksOn "machine learning" .
  }
}
```

### "Who works on machine learning?"
```sparql
SELECT ?person ?label WHERE {
  ?person ex:worksOn "machine learning" ;
          rdfs:label ?label .
}
```

### "What do I know about Sarah?"
```sparql
SELECT ?predicate ?object WHERE {
  ex:Sarah ?predicate ?object .
}
```
```

---

## 6. Integration Patterns

### 6.1 Pattern: Structured Memory (Replace MEMORY.md)

**Current OpenClaw flow**:
```
User says "Sarah is an ML engineer at Acme Corp" →
Agent writes to MEMORY.md: "Sarah is an ML engineer at Acme Corp" →
Later: memory_search("Sarah") → BM25/vector match on text
```

**With pg_ripple**:
```
User says "Sarah is an ML engineer at Acme Corp" →
Agent calls kg_store: three triples →
  ex:Sarah rdf:type ex:Person .
  ex:Sarah ex:role "ML engineer" .
  ex:Sarah ex:worksAt ex:AcmeCorp .
Later: kg_query("SELECT ?role ?org WHERE { ex:Sarah ex:role ?role ; ex:worksAt ?org }")
  → Precise structured answer: "ML engineer at Acme Corp"
```

**Advantages**:
- Facts are queryable by any predicate, not just text similarity
- "Who works at Acme Corp?" returns Sarah without needing to mention her name
- Datalog can derive: `?person ex:colleagueOf ?other :- ?person ex:worksAt ?org, ?other ex:worksAt ?org.`

### 6.2 Pattern: Dreaming Enhancement

OpenClaw's dreaming system promotes short-term signals to `MEMORY.md` based on frequency, relevance, and diversity scores. pg_ripple can provide a **structural signal**:

```
Dreaming deep phase:
  1. Standard frequency/relevance scoring
  2. NEW: Check if the candidate connects to existing KG entities
     - Candidate mentions "Sarah" → ex:Sarah already in KG with 15 facts
     - High connectivity score → boost promotion likelihood
  3. NEW: If promoted, also assert as RDF triple with provenance
     << ex:PromotedFact123 >> ex:promotedFrom "dreaming/deep" ;
                                ex:promotedAt "2026-04-18" ;
                                ex:score 0.87 .
```

### 6.3 Pattern: Memory Wiki Bridge

The `memory-wiki` plugin already compiles durable knowledge into structured claims with `id`, `text`, `status`, `confidence`, `evidence[]`. These map naturally to RDF:

```turtle
# Memory Wiki claim → RDF
wiki:claim_42 a wiki:Claim ;
  wiki:text "Sarah prefers TypeScript over JavaScript" ;
  wiki:status "active" ;
  wiki:confidence 0.85 ;
  wiki:evidence [
    wiki:sourceId "memory/2026-04-15.md" ;
    wiki:lines "12-14" ;
    wiki:weight 0.9 ;
  ] .
```

A bridge mode syncs wiki claims into pg_ripple as RDF triples, enabling SPARQL queries over the wiki's structured beliefs — and SHACL validation to detect contradictory claims.

### 6.4 Pattern: GraphRAG over Personal Data

Full GraphRAG pipeline using pg_ripple as the knowledge graph backend (per [graphrag.md](../graphrag.md)):

```
Step 1: User uploads document to OpenClaw
Step 2: OpenClaw cron job triggers GraphRAG extraction
  - LLM extracts entities + relationships
  - Loaded into pg_ripple as RDF triples
  - pg_ripple runs Datalog inference (derive implicit relationships)
  - pgvector embeddings generated for entities
  - Leiden community detection over entity graph
  - LLM generates community summaries
Step 3: User asks question
  - OpenClaw queries pg_ripple via SPARQL + pg:similar()
  - Hybrid results (structural + semantic) assembled into LLM context
  - Agent generates grounded answer with provenance
```

### 6.5 Pattern: Multi-Agent Shared Knowledge

OpenClaw supports multi-agent routing (different agents per channel/account). With pg_ripple as a shared knowledge backend:

```
Work Agent (Slack):
  - Stores: ex:ProjectAlpha ex:deadline "2026-06-01"^^xsd:date .
  - Stores: ex:ProjectAlpha ex:blocker "waiting on API approval" .

Personal Agent (WhatsApp):
  - Queries: SELECT ?deadline WHERE { ex:ProjectAlpha ex:deadline ?deadline }
  - Can tell user: "Your Project Alpha deadline is June 1st"

Research Agent (Telegram):
  - Queries: SELECT ?project ?blocker WHERE { ?project ex:blocker ?blocker }
  - Reports: "Project Alpha is blocked on API approval"
```

Named graphs provide access control:
```sparql
# Work agent can only read/write work graph
GRAPH <urn:openclaw:agent:work> { ... }

# Personal agent can read all graphs
GRAPH ?g { ... }
```

### 6.6 Pattern: Temporal Reasoning

OpenClaw's daily note files (`memory/YYYY-MM-DD.md`) map naturally to named graphs:

```sparql
# What changed this week?
SELECT ?subject ?predicate ?object ?day
WHERE {
  GRAPH ?g {
    ?subject ?predicate ?object .
  }
  FILTER(STRSTARTS(STR(?g), "urn:openclaw:2026-04-1"))
}
ORDER BY ?day

# When did I first learn about Sarah?
SELECT (MIN(?date) AS ?firstMention)
WHERE {
  GRAPH ?g {
    ex:Sarah ?p ?o .
  }
  BIND(STRAFTER(STR(?g), "urn:openclaw:") AS ?date)
}
```

### 6.7 Pattern: Decision Support with Constraint Tracking

```sparql
# Active constraints on current task
SELECT ?constraint ?priority ?source
WHERE {
  ex:CurrentTask ex:hasConstraint ?c .
  ?c rdfs:label ?constraint ;
     ex:priority ?priority .
  OPTIONAL { ?c ex:source ?source }
  FILTER(?priority > 7)
}
ORDER BY DESC(?priority)

# Derived conflicts via Datalog
?task ex:conflictsWith ?other :-
  ?task ex:requires ?resource,
  ?other ex:requires ?resource,
  ?task ex:deadline ?d1,
  ?other ex:deadline ?d2,
  abs(?d1 - ?d2) < 7.  # Within same week
```

---

## 7. Ontology Design for Personal Knowledge

A minimal, extensible ontology for OpenClaw's personal knowledge domain:

```turtle
@prefix oc: <http://pg-ripple.org/openclaw/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .

# Core classes
oc:Person a rdfs:Class .
oc:Project a rdfs:Class .
oc:Task a rdfs:Class .
oc:Organization a rdfs:Class .
oc:Topic a rdfs:Class .
oc:Event a rdfs:Class .
oc:Preference a rdfs:Class .
oc:Skill a rdfs:Class .
oc:Document a rdfs:Class .
oc:Conversation a rdfs:Class .

# Core properties
oc:name a rdf:Property ; rdfs:range xsd:string .
oc:email a rdf:Property ; rdfs:range xsd:string .
oc:worksAt a rdf:Property ; rdfs:domain oc:Person ; rdfs:range oc:Organization .
oc:worksOn a rdf:Property ; rdfs:domain oc:Person ; rdfs:range oc:Project .
oc:hasSkill a rdf:Property ; rdfs:domain oc:Person ; rdfs:range oc:Skill .
oc:deadline a rdf:Property ; rdfs:range xsd:date .
oc:status a rdf:Property ; rdfs:range xsd:string .
oc:priority a rdf:Property ; rdfs:range xsd:integer .
oc:relatedTo a rdf:Property .  # Symmetric
oc:mentionedIn a rdf:Property ; rdfs:range oc:Conversation .
oc:extractedAt a rdf:Property ; rdfs:range xsd:dateTime .
oc:confidence a rdf:Property ; rdfs:range xsd:float .

# SHACL shapes for quality enforcement
oc:PersonShape a sh:NodeShape ;
  sh:targetClass oc:Person ;
  sh:property [
    sh:path oc:name ;
    sh:minCount 1 ;
    sh:maxCount 1 ;
    sh:datatype xsd:string ;
    sh:maxLength 200 ;
  ] .

oc:TaskShape a sh:NodeShape ;
  sh:targetClass oc:Task ;
  sh:property [
    sh:path oc:status ;
    sh:in ( "todo" "in-progress" "done" "blocked" ) ;
  ] ;
  sh:property [
    sh:path oc:priority ;
    sh:minInclusive 1 ;
    sh:maxInclusive 10 ;
  ] .
```

### 7.1 Datalog Rules for Personal Knowledge

```datalog
# Derive colleague relationships
?a oc:colleagueOf ?b :-
  ?a oc:worksAt ?org,
  ?b oc:worksAt ?org,
  ?a != ?b.

# Derive expertise from project work
?person oc:expertIn ?topic :-
  ?person oc:worksOn ?project,
  ?project oc:hasTopic ?topic.

# Derive overdue tasks
?task oc:isOverdue true :-
  ?task rdf:type oc:Task,
  ?task oc:deadline ?d,
  ?task oc:status ?s,
  ?s != "done",
  ?d < NOW().

# Derive potential collaborators
?a oc:couldCollaborateWith ?b :-
  ?a oc:expertIn ?skill,
  ?b oc:needsExpertise ?skill,
  ?a != ?b.

# Transitive organizational hierarchy
?person oc:worksUnder ?boss :-
  ?person oc:reportsTo ?boss.
?person oc:worksUnder ?boss :-
  ?person oc:reportsTo ?mid,
  ?mid oc:worksUnder ?boss.
```

---

## 8. Deployment Topology

### 8.1 Local Single-Machine (Recommended Default)

```
┌──────────────────────────────────────────────┐
│  User's Machine                              │
│                                              │
│  OpenClaw Gateway (Node.js, port 18789)      │
│       ↕ HTTP                                 │
│  pg_ripple_http (Rust, port 7878)            │
│       ↕ libpq                                │
│  PostgreSQL 18 + pg_ripple (port 5432)       │
│       + pgvector (optional)                  │
└──────────────────────────────────────────────┘
```

**Setup**: Single `docker-compose.yml` or local install. PostgreSQL + pg_ripple + pg_ripple_http run as a unit. OpenClaw plugin connects to `http://127.0.0.1:7878`.

### 8.2 Docker Compose

```yaml
services:
  postgres:
    image: pg_ripple:18
    environment:
      POSTGRES_PASSWORD: openclaw
      POSTGRES_DB: openclaw_kg
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data

  pg-ripple-http:
    image: pg_ripple_http:latest
    environment:
      PG_RIPPLE_HTTP_PG_URL: postgresql://postgres:openclaw@postgres/openclaw_kg
      PG_RIPPLE_HTTP_AUTH_TOKEN: ${KG_AUTH_TOKEN}
      PG_RIPPLE_HTTP_PORT: 7878
    ports:
      - "7878:7878"
    depends_on:
      - postgres

volumes:
  pgdata:
```

### 8.3 Remote / Tailscale

For OpenClaw running on a Mac Mini with pg_ripple on a separate server:

```
Mac Mini (OpenClaw) ──── Tailscale ──── Server (pg_ripple)
                    HTTPS/SPARQL
```

`pg_ripple_http` binds to the Tailscale interface. OpenClaw plugin connects via `https://pg-ripple-server.tail12345.ts.net:7878`.

---

## 9. Performance Considerations

### 9.1 Latency Budget

OpenClaw's agent loop targets sub-second tool execution. pg_ripple_http adds:

| Operation | Expected latency | Acceptable for |
|---|---|---|
| Simple SPARQL SELECT (indexed) | 1–5ms | Every agent turn (context injection) |
| SPARQL with 3+ joins | 5–50ms | On-demand queries |
| SPARQL UPDATE (single triple) | 2–10ms | Fact storage |
| Bulk SPARQL UPDATE (100 triples) | 50–200ms | Post-compaction extraction |
| Datalog inference (small ruleset) | 100–500ms | Background service |
| pg:similar() vector search | 5–20ms | Hybrid retrieval |
| JSON-LD CONSTRUCT + framing | 10–50ms | Context assembly |

The `before_prompt_build` hook must stay under ~100ms to avoid noticeable agent latency. Simple entity lookups and 1-hop neighborhood queries easily meet this.

### 9.2 Scaling Characteristics

Personal knowledge graphs are small by database standards:

| Metric | Typical personal KG | pg_ripple sweet spot |
|---|---|---|
| Triples | 10K–1M | Up to 100M+ |
| Entities | 1K–100K | Up to 10M+ |
| Predicates | 50–500 | Up to 100K+ |
| Named graphs | 365+ (daily) | Unlimited |
| Concurrent sessions | 1–5 | MVCC handles many |

pg_ripple's VP storage and HTAP architecture are massively overprovisioned for personal use — which means the system stays fast even as the knowledge graph grows over years.

---

## 10. Phased Delivery Plan

### Phase 1: Skill-Only Integration (No Plugin Required)

**Effort**: 1–2 days
**Prerequisites**: pg_ripple_http running, OpenClaw workspace

Deliverables:
- `skills/knowledge-graph/SKILL.md` — teaches agent to use curl + pg_ripple_http
- `skills/knowledge-graph/ontology.ttl` — base ontology
- Setup guide in OpenClaw docs

This gets value immediately: the agent can store and query facts via the skill's instructions, using `exec` to run curl commands. No TypeScript plugin needed.

### Phase 2: Native Plugin (Tools + Hooks)

**Effort**: 2–3 weeks
**Prerequisites**: Phase 1, familiarity with OpenClaw plugin SDK

Deliverables:
- `@openclaw/pg-ripple` npm package
- Agent tools: `kg_query`, `kg_store`, `kg_facts`, `kg_similar`, `kg_infer`, `kg_validate`
- Lifecycle hooks: `before_prompt_build`, `before_compaction`, `after_tool_call`
- Memory corpus supplement (corpus=kg)
- CLI subcommands: `openclaw kg status`, `openclaw kg query`, `openclaw kg import`
- Docker Compose template

### Phase 3: Dreaming + Wiki Bridge

**Effort**: 1–2 weeks
**Prerequisites**: Phase 2, memory-wiki plugin

Deliverables:
- Dreaming hook: structural connectivity scoring for promotion candidates
- Wiki bridge: sync wiki claims → RDF triples
- SHACL shapes for wiki claims
- Contradiction detection via SPARQL across wiki + KG

### Phase 4: GraphRAG Pipeline

**Effort**: 3–4 weeks
**Prerequisites**: Phase 2, pg_ripple v0.26.0+ (GraphRAG integration), v0.27.0+ (pgvector)

Deliverables:
- Document ingestion cron job (extract entities → pg_ripple)
- Leiden community detection integration
- Community summary generation
- Hybrid SPARQL + vector retrieval for RAG
- End-to-end test: upload document → query → grounded answer

### Phase 5: Advanced Features

**Effort**: Ongoing
**Prerequisites**: Phase 4

Deliverables:
- Multi-agent knowledge sharing with graph-level access control
- Temporal reasoning (named graph per day)
- Graph-contextualized embeddings for improved vector search
- SHACL-validated extraction pipeline
- Federation with external knowledge bases (Wikidata, DBpedia)
- OpenClaw ClawHub publication

---

## 11. Competitive Landscape

### 11.1 Existing OpenClaw Memory Solutions

| Solution | Strengths | Limitations vs pg_ripple |
|---|---|---|
| **memory-core** (builtin) | Zero setup, hybrid search, dreaming | Flat text, no structural queries, no reasoning |
| **QMD** | Reranking, query expansion, extra paths | Still document-oriented, no graph traversal |
| **Honcho** | Cross-session, user modeling | External service, no SPARQL, no reasoning |
| **memory-wiki** | Structured claims, provenance, dashboards | Markdown-based, no join queries, no inference |

pg_ripple doesn't replace any of these — it **supplements** them as a structured knowledge layer. The plugin registers as a memory corpus supplement, so `memory_search corpus=all` queries both the existing memory backend *and* the knowledge graph.

### 11.2 Alternative Graph Backends for OpenClaw

| Alternative | Why pg_ripple is better |
|---|---|
| Neo4j | No SPARQL, no SHACL, no Datalog, separate infrastructure |
| SQLite + JSON | No graph query language, no reasoning, no validation |
| Custom Python graph | No standards, no optimization, fragile |
| Weaviate/Qdrant | Vector-only, no structural reasoning, no provenance |
| LangChain graph stores | Thin wrappers, limited query capabilities |

pg_ripple's unique position: **the only system that runs inside PostgreSQL with full SPARQL 1.1, SHACL, Datalog, and pgvector integration in a single process**.

---

## 12. User Stories

### 12.1 "Personal CRM"

> As an OpenClaw user, I want my assistant to remember everyone I've mentioned — their roles, organizations, and relationships — so I can ask "Who at Acme Corp could help with the database migration?" and get a precise answer.

**Flow**: Conversations → auto-extraction → RDF triples → Datalog derives `ex:colleagueOf` and `ex:expertIn` → SPARQL query → grounded answer.

### 12.2 "Project Dashboard"

> As a project manager using OpenClaw, I want to ask "What tasks are overdue?" and get a structured list with deadlines and assignees, not a fuzzy text search result.

**Flow**: Tasks stored as typed RDF entities → Datalog derives `oc:isOverdue` → SPARQL query with date filters → formatted table.

### 12.3 "Research Assistant"

> As a researcher, I want to upload papers and ask questions that connect findings across documents — "Which studies contradict the finding that X causes Y?"

**Flow**: GraphRAG extraction → community detection → SPARQL federation with external knowledge bases → SHACL-validated fact graph → hybrid retrieval.

### 12.4 "Second Brain"

> As a knowledge worker, I want everything I tell my assistant to be queryable, connected, and automatically enriched with inferences — not lost in a pile of daily note files.

**Flow**: Every conversation → fact extraction → SHACL validation → Datalog inference → persistent, queryable knowledge graph that grows more useful over time.

---

## 13. Open Questions

1. **Entity resolution**: How should the plugin handle when the user says "Sarah" in one conversation and "Sarah Chen" in another? Options: LLM-assisted entity linking, SHACL `sh:uniqueLang` constraints, `owl:sameAs` assertions.

2. **Privacy boundaries**: Named graphs provide coarse access control, but some users may want per-entity privacy levels. Consider SHACL-based access control shapes.

3. **Embedding model alignment**: OpenClaw's memory search uses one embedding model; pg_ripple's pgvector may use another. Should they share? The plugin could use the same provider configured for OpenClaw memory search.

4. **Plugin vs MCP**: OpenClaw supports MCP (Model Context Protocol) servers. Should pg_ripple also expose an MCP interface alongside the SPARQL endpoint? This would make it usable by any MCP-compatible agent, not just OpenClaw.

5. **Ontology evolution**: As users store more diverse knowledge, the base ontology needs to grow. Should the plugin auto-suggest new classes/properties, or require explicit ontology extensions?

6. **Deduplication**: When both memory-core and the KG plugin extract the same fact, how to avoid duplicates? The memory corpus supplement should tag KG-sourced results so the agent doesn't re-store them.

---

## 14. Summary

| What pg_ripple gives OpenClaw | How |
|---|---|
| **Structured memory** | RDF triples replace/supplement text blobs |
| **Precise recall** | SPARQL queries return exact fact tuples, not fuzzy text matches |
| **Automatic reasoning** | Datalog rules derive new facts (colleagues, expertise, conflicts) |
| **Knowledge quality** | SHACL shapes reject malformed or contradictory facts |
| **Semantic + structural search** | pgvector embeddings + SPARQL in a single query (v0.27.0+) |
| **Temporal reasoning** | Named graphs per day/session enable "what changed?" queries |
| **Provenance tracking** | RDF-star statement-level metadata: who said what, when, with what confidence |
| **GraphRAG pipeline** | Full personal knowledge base with community detection and hybrid retrieval |
| **Multi-agent coordination** | Shared knowledge graph with graph-level access control |
| **Standards compliance** | W3C SPARQL 1.1, SHACL, RDF-star — not a proprietary format |

| What OpenClaw gives pg_ripple | How |
|---|---|
| **Consumer distribution** | 360k-star project with active user base |
| **Natural language interface** | Users interact via chat, not SPARQL |
| **Multi-channel reach** | Knowledge graph accessible from WhatsApp, Telegram, Slack, etc. |
| **LLM-powered extraction** | Conversations → structured triples automatically |
| **Automation** | Cron jobs, webhooks, hooks for background knowledge maintenance |
| **Plugin marketplace** | ClawHub distribution for the pg_ripple plugin |

The combination is **an AI agent that remembers, reasons, and knows when it doesn't know** — backed by a production-grade, standards-compliant knowledge engine running inside PostgreSQL.
