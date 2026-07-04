#!/usr/bin/env python3
"""Regroup `impl Widget for <T>` blocks in a file into full-path capability impls.
Usage: migrate.py <file>
- Processes EVERY `impl Widget for <T>` block in the file.
- Emits `impl crate::widgets::<Cap> for <T>` blocks (full paths -> no imports needed).
- Removes `impl Renderable for <T>` blocks (macro regenerates them).
- Prints the `#[widget(<caps>)]` attribute each type needs.
Does NOT touch imports/struct attrs/node refs (Widget stays imported -> bare calls forward).
"""
import re, sys

RENDER=["render","render_with_debug","compose","render_line","render_lines","style_type","style_type_aliases","border_title","border_subtitle"]
INTERACTIVE=["on_mount","on_unmount","on_tick","on_resize","on_layout","on_event_capture","on_event","on_message","on_mouse_move","on_node_state_changed"]
LAYOUT=["content_width","auto_content_width","layout_height","auto_content_height","set_virtual_content_size","tree_child_content_inset","child_display_for_tree","child_classes_for_tree","is_transparent_wrapper","preserve_underlay","clips_descendants_to_content","style"]
SCROLLABLE=["scroll_offset","scroll_offset_f32","scroll_viewport_size","scroll_virtual_content_size","on_mouse_scroll"]
FOCUS=["focusable","can_focus","can_focus_children","mouse_interactive","is_active","is_initially_disabled","is_initially_focused","bindings","binding_hints","action_namespace","action_registry","execute_action","check_action","help_markup"]
SELECTABLE=["allow_select","selection_at","selection_word_range_at","selection_all_range","update_selection","clear_selection","get_selection","selection_updated"]
HASTOOLTIP=["tooltip","tooltip_anchor"]
COMPONENTS=["component_classes","get_component_styles","get_component_rich_style"]
APPHOOKS=["on_app_key","on_app_action","on_app_unhandled_action","on_app_message","on_app_tick","on_app_timer","on_app_mount"]
STYLEID=["style_classes","style_id","is_hovered","set_seed_css_id","set_seed_classes"]
SEED=["take_node_seed","set_inline_style"]
GROUPS=[("Render",RENDER),("Interactive",INTERACTIVE),("Layout",LAYOUT),("Scrollable",SCROLLABLE),
        ("Focus",FOCUS),("Selectable",SELECTABLE),("HasTooltip",HASTOOLTIP),("Components",COMPONENTS),
        ("AppHooks",APPHOOKS),("StyleIdentity",STYLEID)]
NAME2GROUP={}
for g,ns in GROUPS:
    for n in ns: NAME2GROUP[n]=g
for n in SEED: NAME2GROUP[n]="Seed"

CANON_TAKE="std::mem::take(&mut self.seed)"
CANON_SET="self.seed.styles.style = style"

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
        # capture preceding doc/attr lines
        before=body[:ls].rstrip("\n").split("\n"); pf=[]; bi=len(before)-1
        while bi>=0 and (before[bi].strip().startswith("///") or before[bi].strip().startswith("//") or before[bi].strip().startswith("#[")):
            pf.insert(0,before[bi]); bi-=1
        src=("\n".join(pf)+"\n" if pf else "")+body[ls:k+1]
        methods.append((name,src)); j=k+1
    return methods

def process_type(s, ty):
    header=f"impl Widget for {ty} "
    if header not in s:
        header=f"impl Widget for {ty}\n"  # rare
    i,k,body=match_block(s,f"impl Widget for {ty} ")
    ms=parse_methods(body)
    bygroup={}; caps=set(); framework=[]
    for name,src in ms:
        g=NAME2GROUP.get(name,"Framework")
        if g=="Framework":
            if name=="reactive_widget": caps.add("__reactive__"); continue
            framework.append(name); continue
        bygroup.setdefault(g,[]).append((name,src))
    if framework:
        raise SystemExit(f"[{ty}] overrides Framework-group methods (need iteration/escape-hatch): {framework}")
    # seed handling
    seed_ms=bygroup.pop("Seed",[])
    styleid_present="StyleIdentity" in bygroup
    noncanon=False
    for name,src in seed_ms:
        body_txt=src[src.index("{")+1:src.rindex("}")]
        if name=="take_node_seed" and CANON_TAKE not in body_txt: noncanon=True
        if name=="set_inline_style" and CANON_SET not in body_txt: noncanon=True
    if seed_ms:
        if styleid_present or noncanon:
            bygroup.setdefault("StyleIdentity",[]).extend(seed_ms)  # StyleIdentity owns seed
        # else: drop (autowired)
    # caps for attr = optional groups present
    for g in bygroup:
        if g!="Render": caps.add(g)
    # emit blocks in canonical order
    order=["Focus","Interactive","Layout","Scrollable","Selectable","HasTooltip","Components","AppHooks","StyleIdentity","Render"]
    blocks=[]
    for g in order:
        if g in bygroup:
            body_src="\n\n".join(src.rstrip() for _,src in bygroup[g])
            blocks.append(f"impl crate::widgets::{g} for {ty} {{\n{body_src}\n}}")
    new_impl="\n\n".join(blocks)
    s=s[:i]+new_impl+s[k+1:]
    # remove impl Renderable for ty
    rh=f"impl Renderable for {ty} "
    if rh in s:
        ri,rk,_=match_block(s,rh)
        after=s[rk+1:]
        s=s[:ri].rstrip("\n")+"\n"+after.lstrip("\n")
    # build attr
    attr_caps=[]
    for g in ["Focus","Interactive","Layout","Scrollable","Selectable","HasTooltip","Components","AppHooks","StyleIdentity"]:
        if g in caps: attr_caps.append(g)
    reactive=" reactive," if "__reactive__" in caps else ""
    attr=f"#[widget({', '.join(attr_caps)}{reactive})]"
    return s, ty, attr

def main():
    path=sys.argv[1]
    only=sys.argv[2].split(",") if len(sys.argv)>2 else None
    s=open(path).read()
    types=re.findall(r"impl Widget for (\w+)\b", s)
    seen=[]; [seen.append(t) for t in types if t not in seen]
    todo=[t for t in seen if (only is None or t in only)]
    print(f"== {path}: migrating {todo}")
    for ty in todo:
        s,ty,attr=process_type(s,ty)
        # strip delegate_renderable!(ty); the #[widget] derive regenerates Renderable.
        s=re.sub(rf"(?m)^\s*delegate_renderable!\({ty}\);\s*\n","",s)
        print(f"   {ty}: {attr}")
    open(path,"w").write(s)

main()
