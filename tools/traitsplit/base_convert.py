#!/usr/bin/env python3
"""Convert a `delegate_widget_method!` delegation widget to `#[widget(base=field)]`.
Usage: base_convert.py <file> <Type> <field> <BaseType>
- Moves the OWN `fn` methods from `impl Widget for <Type>` into the first
  `impl <Type> {` block as inherent methods (skips names already inherent there).
- Adds inherent delegating `style_type`/`style_type_aliases` if they were in the
  delegate list (base= does not forward those by default).
- Deletes the `impl Widget` block, `delegate_renderable!(<Type>)`, and any hand
  `impl Renderable for <Type>`.
- Prints the `#[widget(base=..)]` attribute + any collision notes.
Then MANUALLY: add the attr before the struct, add `use textual_macros::widget;`,
remove the delegate imports, build + test.
"""
import re, sys

def match_block(s, header):
    i=s.index(header); b=s.index("{",i); d=0; k=b
    while k<len(s):
        c=s[k]
        if c=='{': d+=1
        elif c=='}':
            d-=1
            if d==0: break
        k+=1
    return i,k,s[b+1:k]

def parse_methods(body):
    methods=[]; j=0
    while True:
        idx=body.find("\n    fn ", j)
        if idx==-1: break
        ls=idx+1
        name=re.match(r"    fn (\w+)", body[ls:]).group(1)
        br=body.index("{", ls); d=0; k=br
        while k<len(body):
            c=body[k]
            if c=='{': d+=1
            elif c=='}':
                d-=1
                if d==0: break
            k+=1
        before=body[:ls].rstrip("\n").split("\n"); pf=[]; bi=len(before)-1
        while bi>=0 and (before[bi].strip().startswith("///") or before[bi].strip().startswith("//") or before[bi].strip().startswith("#[")):
            pf.insert(0,before[bi]); bi-=1
        src=("\n".join(pf)+"\n" if pf else "")+body[ls:k+1]
        methods.append((name,src)); j=k+1
    return methods

def main():
    path,ty,field,base=sys.argv[1:5]
    s=open(path).read()
    wi,wk,wbody=match_block(s,f"impl Widget for {ty} ")
    own=parse_methods(wbody)
    own_names=[n for n,_ in own]
    # delegate list
    dl=re.search(r"delegate_widget_method!\(\s*\w+,\s*\[(.*?)\]\s*\);", wbody, re.S)
    delegated=[x.strip() for x in dl.group(1).split(",") if x.strip()] if dl else []
    # existing inherent methods in `impl <ty> {` (first non-Widget impl block)
    inh_i,inh_k,inh_body=match_block(s,f"impl {ty} {{" if f"impl {ty} {{" in s else f"impl {ty} ")
    existing_inh={n for n,_ in parse_methods(inh_body)}
    # methods to add: own not already inherent
    to_add=[(n,src) for n,src in own if n not in existing_inh]
    # style_type / style_type_aliases delegated-but-not-forwarded-by-base
    deleg_overrides=[]
    for m in ("style_type","style_type_aliases"):
        if m in delegated and m not in own_names and m not in existing_inh:
            deleg_overrides.append(m)
    add_src=""
    for n,src in to_add:
        add_src+="\n"+src.rstrip()+"\n"
    if "style_type" in deleg_overrides:
        add_src+=f"\n    fn style_type(&self) -> &'static str {{ self.{field}.style_type() }}\n"
    if "style_type_aliases" in deleg_overrides:
        add_src+=f"\n    fn style_type_aliases(&self) -> &[&'static str] {{ self.{field}.style_type_aliases() }}\n"
    # insert into inherent block (recompute indices on fresh string ops)
    s=s[:inh_k]+add_src+s[inh_k:]
    # delete impl Widget block (indices shifted only if impl Widget is AFTER inherent block; recompute)
    wi,wk,_=match_block(s,f"impl Widget for {ty} ")
    s=s[:wi]+s[wk+1:]
    # delete delegate_renderable + hand impl Renderable
    s=re.sub(rf"(?m)^\s*delegate_renderable!\({ty}\);\s*\n","",s)
    if f"impl Renderable for {ty} " in s:
        ri,rk,_=match_block(s,f"impl Renderable for {ty} ")
        s=s[:ri].rstrip("\n")+"\n"+s[rk+1:].lstrip("\n")
    s=re.sub(r"\n\n\n+","\n\n",s)
    open(path,"w").write(s)
    override_list=own_names+deleg_overrides
    print(f"== {ty}: base={base} field={field}")
    print(f"   #[widget(base = {base}, field = {field}, override({', '.join(override_list)}))]")
    print(f"   moved {len(to_add)} own methods to inherent; skipped (already inherent): {[n for n in own_names if n in existing_inh]}")
    print(f"   delegated-overrides added: {deleg_overrides}")

main()
