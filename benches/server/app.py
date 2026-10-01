from fastapi import FastAPI
app = FastAPI()
@app.get("/health")
def health():
    return {"ok": True}
@app.get("/users/{id}")
def user(id: int):
    return {"id": id, "name": f"user-{id}"}
