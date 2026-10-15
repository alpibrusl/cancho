import yaml,glob,os,re,subprocess,sys,json,time
HERE=os.path.dirname(os.path.abspath(__file__))
CANCHO=os.environ["CANCHO_BIN"]; CC=os.environ.get("CC_WRAP","")
SKIP=re.compile(r"apt-get|apt |pip install|pip3 install|sudo|cargo |rustup|curl |wget |docker|psql|postgres|redis-server|redis-cli|npm |node |git clone|gh |checkout|nginx|haproxy|tcpdump|openssl s_|strace|perf ")
BIN=re.compile(r"(\.\./cancho/target/release/cancho|\$\{CANCHO\}|\$CANCHO\b|(?<![\w/.\-])cancho(?= (?:build|test|check|install|ids|fmt|authority|layout|vcs)))")
out=open(os.path.join(HERE,"e12b_results.jsonl"),"a")
def steps(repo):
    for yml in sorted(glob.glob(f"{HERE}/{repo}/.github/workflows/*.yml")):
        doc=yaml.safe_load(open(yml))
        for jname,job in (doc.get("jobs") or {}).items():
            for st in job.get("steps",[]):
                if "run" in st: yield os.path.basename(yml),jname,st.get("name",""),st["run"],st.get("env",{})
def prepare(repo,name,run):
    why=None
    if SKIP.search(run): why="needs a service or tool outside the compiler (matched: %s)"%SKIP.search(run).group(0).strip()
    elif not (BIN.search(run) or re.search(r"python3 tests/|scripts/",run)): why="not a build/test step"
    return why
for repo in sys.argv[1:]:
    os.makedirs(f"{HERE}/{repo}/build",exist_ok=True)
    for yml,job,name,run,env in steps(repo):
        why=prepare(repo,name,run)
        rec={"repo":repo,"job":job,"step":name[:70]}
        if why:
            rec.update(result="skipped",why=why); out.write(json.dumps(rec)+"\n"); out.flush(); continue
        script=BIN.sub(CANCHO,run)
        variants=[("as-written",script)]
        if re.search(r"cancho\w*[\s/]*(build|test|install)\b",run) and not re.search(r"\.cho|--std",run):
            variants.append(("ignore-compiler-rev",re.sub(r"(%s)( )(build|test)"%re.escape(CANCHO),r"\1 \3 --ignore-compiler-rev",script)))
        for vname,sc in variants:
            e=dict(os.environ); e["CANCHO"]=CANCHO
            if CC: e["CC"]=CC
            for k,v in (env or {}).items(): e[k]=str(v)
            t=time.time()
            try:
                r=subprocess.run(["bash","-ec",sc],cwd=f"{HERE}/{repo}",capture_output=True,text=True,timeout=900,env=e)
                rc=r.returncode; tail=(r.stdout+r.stderr).strip().splitlines()[-2:]
            except subprocess.TimeoutExpired:
                rc="timeout"; tail=[]
            rec2=dict(rec,variant=vname,rc=rc,sec=round(time.time()-t,1),tail=[x[:140] for x in tail])
            out.write(json.dumps(rec2)+"\n"); out.flush()
            print(repo,"|",name[:50],"|",vname,"|",rc,flush=True)
