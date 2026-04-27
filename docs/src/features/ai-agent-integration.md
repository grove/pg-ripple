# AI Agent Integration (LangChain, LlamaIndex, and Friends)

pg_ripple's AI capabilities — vector search, `rag_context()`, `sparql_from_nl()`, graph expansion — are exposed as **plain SQL functions**. Any framework that can call PostgreSQL can use them. No SDK required.

This page shows how to wire pg_ripple into the two most common Python agent frameworks, plus a framework-agnostic tool-calling pattern.

---

## The mental model

An AI agent loop looks like this:

```
user question
     │
     ▼
   LLM decides: do I need to look something up?
     │
     ├── yes → call a tool (pg_ripple function via SQL) → get context
     │          └── loop back to LLM with context
     │
     └── no  → generate final answer
```

pg_ripple is the tool. The LLM calls it via `rag_context()`, `sparql_from_nl()`, or a custom SQL wrapper — whichever your framework exposes.

---

## Prerequisites

```sql
-- Configure the embedding endpoint (once per database).
ALTER SYSTEM SET pg_ripple.embedding_api_url     = 'https://api.openai.com/v1';
ALTER SYSTEM SET pg_ripple.embedding_api_key_env = 'OPENAI_API_KEY';
ALTER SYSTEM SET pg_ripple.embedding_model       = 'text-embedding-3-small';

ALTER SYSTEM SET pg_ripple.llm_endpoint          = 'https://api.openai.com/v1';
ALTER SYSTEM SET pg_ripple.llm_api_key_env       = 'OPENAI_API_KEY';
ALTER SYSTEM SET pg_ripple.llm_model             = 'gpt-4o';

SELECT pg_reload_conf();

-- Embed your knowledge graph (once, then incrementally as new triples arrive).
SELECT pg_ripple.embed_entities();
```

---

## LangChain

The cleanest integration is a `Tool` that calls `rag_context()` and a second `Tool` that calls `sparql_from_nl()` for fact-style answers.

```python
from langchain.tools import Tool
from langchain_openai import ChatOpenAI
from langchain.agents import AgentExecutor, create_openai_tools_agent
from langchain_core.prompts import ChatPromptTemplate, MessagesPlaceholder
import psycopg

DB_URL = "postgresql://..."

def graph_context(question: str) -> str:
    """Retrieve graph context for a question from the knowledge graph."""
    with psycopg.connect(DB_URL) as conn:
        cur = conn.cursor()
        cur.execute("SELECT pg_ripple.rag_context(%s, 8)", (question,))
        return cur.fetchone()[0]


def graph_query(sparql: str) -> str:
    """Execute a SPARQL query against the knowledge graph and return results as JSON."""
    with psycopg.connect(DB_URL) as conn:
        cur = conn.cursor()
        cur.execute("SELECT jsonb_agg(row_to_json(r)) FROM pg_ripple.sparql(%s) r", (sparql,))
        result = cur.fetchone()[0]
        return str(result) if result else "No results."


def nl_to_sparql_and_run(question: str) -> str:
    """Convert a natural-language question to SPARQL and run it."""
    with psycopg.connect(DB_URL) as conn:
        cur = conn.cursor()
        cur.execute(
            "SELECT jsonb_agg(row_to_json(r)) FROM "
            "pg_ripple.sparql(pg_ripple.sparql_from_nl(%s)) r",
            (question,)
        )
        result = cur.fetchone()[0]
        return str(result) if result else "No results."


tools = [
    Tool(name="graph_context",       func=graph_context,
         description="Retrieve rich graph context for open-ended questions."),
    Tool(name="nl_to_sparql_query",  func=nl_to_sparql_and_run,
         description="Answer precise factual questions using SPARQL auto-generation."),
]

llm    = ChatOpenAI(model="gpt-4o", temperature=0)
prompt = ChatPromptTemplate.from_messages([
    ("system", "You are a helpful assistant with access to a knowledge graph. "
               "Use 'graph_context' for broad questions and 'nl_to_sparql_query' for specific facts."),
    ("human", "{input}"),
    MessagesPlaceholder("agent_scratchpad"),
])
agent    = create_openai_tools_agent(llm, tools, prompt)
executor = AgentExecutor(agent=agent, tools=tools, verbose=True)

result = executor.invoke({"input": "Which drugs interact with insulin and should be avoided?"})
print(result["output"])
```

---

## LlamaIndex

LlamaIndex's `FunctionTool` maps directly onto pg_ripple SQL calls.

```python
from llama_index.core.tools import FunctionTool
from llama_index.core.agent import ReActAgent
from llama_index.llms.openai import OpenAI
import psycopg

DB_URL = "postgresql://..."


def retrieve_graph_context(question: str) -> str:
    """
    Search the knowledge graph for entities and relationships relevant to the question.
    Returns a structured text block suitable for LLM prompting.
    """
    with psycopg.connect(DB_URL) as conn:
        cur = conn.cursor()
        cur.execute("SELECT pg_ripple.rag_context(%s, 8)", (question,))
        return cur.fetchone()[0]


def sparql_fact_lookup(question: str) -> str:
    """
    Answer a precise factual question by auto-generating and executing a SPARQL query.
    Use this for questions like 'how many', 'list all', 'who is', 'what is'.
    """
    with psycopg.connect(DB_URL) as conn:
        cur = conn.cursor()
        cur.execute(
            "SELECT jsonb_agg(row_to_json(r)) FROM "
            "pg_ripple.sparql(pg_ripple.sparql_from_nl(%s)) r",
            (question,)
        )
        result = cur.fetchone()[0]
        return str(result) if result else "Query returned no results."


tools = [
    FunctionTool.from_defaults(fn=retrieve_graph_context),
    FunctionTool.from_defaults(fn=sparql_fact_lookup),
]

agent = ReActAgent.from_tools(tools, llm=OpenAI(model="gpt-4o"), verbose=True)
response = agent.chat("What are the known side effects of combining metformin with insulin?")
print(response)
```

---

## Framework-agnostic: OpenAI tool-calling

If you are not using LangChain or LlamaIndex, use OpenAI's tool-calling API directly. This works with any framework or plain Python.

```python
import json, psycopg, openai

DB_URL = "postgresql://..."
client = openai.OpenAI()

TOOLS = [
    {
        "type": "function",
        "function": {
            "name": "rag_context",
            "description": "Retrieve relevant context from the knowledge graph for an open-ended question.",
            "parameters": {
                "type": "object",
                "properties": {
                    "question": {"type": "string"},
                    "k":        {"type": "integer", "default": 8,
                                 "description": "Number of entities to retrieve."}
                },
                "required": ["question"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "sparql_fact_lookup",
            "description": "Answer a precise factual question using auto-generated SPARQL.",
            "parameters": {
                "type": "object",
                "properties": {"question": {"type": "string"}},
                "required": ["question"],
            },
        },
    },
]


def dispatch(tool_name: str, args: dict) -> str:
    with psycopg.connect(DB_URL) as conn:
        cur = conn.cursor()
        if tool_name == "rag_context":
            cur.execute("SELECT pg_ripple.rag_context(%s, %s)", (args["question"], args.get("k", 8)))
            return cur.fetchone()[0]
        elif tool_name == "sparql_fact_lookup":
            cur.execute(
                "SELECT jsonb_agg(row_to_json(r)) FROM "
                "pg_ripple.sparql(pg_ripple.sparql_from_nl(%s)) r",
                (args["question"],)
            )
            result = cur.fetchone()[0]
            return str(result) if result else "No results."
    return "Unknown tool."


def agent_loop(question: str, max_turns: int = 5) -> str:
    messages = [{"role": "user", "content": question}]
    for _ in range(max_turns):
        response = client.chat.completions.create(
            model="gpt-4o", messages=messages, tools=TOOLS, tool_choice="auto"
        )
        msg = response.choices[0].message
        messages.append(msg)
        if msg.tool_calls:
            for call in msg.tool_calls:
                result = dispatch(call.function.name, json.loads(call.function.arguments))
                messages.append({
                    "role": "tool", "tool_call_id": call.id, "content": result
                })
        else:
            return msg.content
    return "Max turns reached."


print(agent_loop("Which drugs interact with insulin and should be avoided?"))
```

---

## Tips for production agents

- **Cache `rag_context()` results** per question hash (most questions repeat). A Redis or PostgreSQL cache in front of the tool call cuts LLM costs and latency significantly.
- **Set `pg_ripple.sparql_query_timeout`** to bound runaway auto-generated SPARQL queries.
- **Use few-shot examples** (`pg_ripple.add_llm_example()`) for domain-specific vocabularies — reduces the NL→SPARQL error rate dramatically for specialised graphs.
- **Log tool call results** to `_pg_ripple.audit_log` (enabled by default when `audit_log_enabled = on`) — every RAG retrieval is then auditable, which matters for regulated industries.
- **Multi-tenant**: apply [graph RLS](multi-tenant-graphs.md) on the PostgreSQL connection used by each tenant's agent session — `rag_context()` respects RLS automatically.

---

## What is coming in v1.1.0

- Native LangChain `BaseTool` and LlamaIndex `QueryEngineTool` wrappers published to PyPI as `pg-ripple-langchain` and `pg-ripple-llamaindex`.
- Streaming tool results via pg_ripple cursor API (useful for large graph context blocks).
- Graph-aware conversation memory: store conversation history as RDF triples so the agent can reason over past interactions.

---

## See also

- [AI Overview](ai-overview.md) — which AI feature to use when.
- [RAG Pipeline](../user-guide/rag-pipeline.md) — `rag_context()` deep dive.
- [NL → SPARQL](nl-to-sparql.md) — `sparql_from_nl()` and few-shot tuning.
- [Cookbook: Grounded Chatbot](../cookbook/grounded-chatbot.md) — simpler single-agent recipe.
