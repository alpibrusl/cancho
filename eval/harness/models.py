"""Providers. Keys are read from the environment and never written anywhere. Every call returns (text, usage)."""
import json, os, subprocess, urllib.request

def _anthropic_key():
    """The key, from the file cancho-code reads it from (`ANTHROPIC_KEY_FILE`, default ~/.config/cancho-code/anthropic.key);
    never printed, never written anywhere."""
    path = os.environ.get("ANTHROPIC_KEY_FILE", os.path.expanduser("~/.config/cancho-code/anthropic.key"))
    with open(path) as f: return f.read().strip()

def anthropic_chat(model, system, messages, max_tokens=8000):
    payload = dict(model=model, max_tokens=max_tokens, thinking={"type": "disabled"}, messages=messages,
                   system=[{"type": "text", "text": system, "cache_control": {"type": "ephemeral"}}])
    headers = {"content-type": "application/json", "x-api-key": _anthropic_key(), "anthropic-version": "2023-06-01"}
    workspace = os.environ.get("ANTHROPIC_WORKSPACE_ID")
    if workspace: headers["anthropic-workspace-id"] = workspace
    req = urllib.request.Request("https://api.anthropic.com/v1/messages", json.dumps(payload).encode(), headers)
    with urllib.request.urlopen(req, timeout=300) as r: j = json.load(r)
    text = "".join(b.get("text", "") for b in j["content"])
    u = j.get("usage", {})
    return text, dict(input=u.get("input_tokens", 0) + u.get("cache_creation_input_tokens", 0) + u.get("cache_read_input_tokens", 0),
                      output=u.get("output_tokens", 0))

def ollama_chat(model, system, messages, max_tokens=8000):
    host = os.environ.get("OLLAMA_HOST", "http://localhost:11434")
    body = json.dumps(dict(model=model, stream=False, messages=[dict(role="system", content=system)] + messages,
                           options=dict(num_predict=max_tokens, temperature=0.2))).encode()
    req = urllib.request.Request(host + "/api/chat", body, {"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=1800) as r: j = json.load(r)
    return j["message"]["content"], dict(input=j.get("prompt_eval_count", 0), output=j.get("eval_count", 0))

def opencode_chat(model, system, messages, max_tokens=8000):
    # Through the opencode CLI (it holds its own credentials); the conversation is flattened into one prompt.
    prompt = system + "\n\n" + "\n\n".join(f"[{m['role']}]\n{m['content']}" for m in messages) + \
        "\n\nReply with the complete solution file in one fenced code block. Do not use tools or edit files."
    r = subprocess.run(["opencode", "run", "-m", model, prompt], capture_output=True, text=True, timeout=900)
    return r.stdout, dict(input=0, output=0)

PROVIDERS = dict(anthropic=anthropic_chat, ollama=ollama_chat, opencode=opencode_chat)

def chat(spec, system, messages):
    provider, model = spec.split(":", 1)
    if provider == "stub": return STUB(model, messages)
    return PROVIDERS[provider](model, system, messages)

STUB = lambda model, messages: ("```\n" + open(model).read() + "\n```", dict(input=0, output=0))
