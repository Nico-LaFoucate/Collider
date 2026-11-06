#!/usr/bin/env python3
"""release-webkit-patch.py [--check] LIBWEBKIT -- used by release.sh (Collider#2).

Collider showed a blank window on a Fedora 45 desktop and printed "WebProcess didn't exit as expected after the UI process
connection was closed". WebKit's page process was stuck in its Skia font manager (SkFontMgr_fontconfig, GetFamilyNames),
which reads every installed font's family names like this:

    for (int id = 0; ; ++id) {
        FcResult result = FcPatternGetString(font, FC_FAMILY, id, &name);
        if (result == FcResultNoId) break;      // the ONLY way out
        if (result != FcResultMatch) continue;
        ...
    }

For a font whose family cannot be read, fontconfig answers FcResultNoMatch for every id, so the loop never ends, the page
never draws, and WebKit cannot stop the process when the window closes. The same loop is in Fedora's own WebKitGTK 2.54.

This changes the one `je` after `cmp $0x3,%eax` (the FcResultNoId test) into `jmp`, so any result other than a match ends
that font's names, which is what the loop means to do. The compiler emits the loop twice (system fonts, application fonts):
both are patched, 1 byte each. The script refuses unless it finds exactly those two loops and each calls
FcPatternGetString@plt; a new WebKit build then needs the loop checked by hand. --check only verifies that both are patched.
"""
import re, subprocess, sys

args = sys.argv[1:]
check = args[:1] == ["--check"]
lib = args[-1]
data = bytearray(open(lib, "rb").read())

# cmp $3,%eax; je|jmp rel8; inc %r32; mov disp8(%rbp),%rdi; mov %r64,%rsi; mov %r32,%edx; lea disp8(%rbp),%rcx;
# call rel32; test %eax,%eax; jne rel8 (back to the cmp)
LOOP = re.compile(rb"\x83\xf8\x03[\x74\xeb].\x41\xff[\xc4-\xc7]\x48\x8b\x7d.[\x4c\x48]\x89[\xc6-\xfe]"
                  rb"\x44\x89[\xc2-\xfa]\x48\x8d\x4d.\xe8....\x85\xc0\x75.", re.S)


def disasm(start, end):
    out = subprocess.run(["objdump", "-d", "--no-show-raw-insn", f"--start-address={start}", f"--stop-address={end}", lib],
                         capture_output=True, text=True, check=True).stdout
    return [l.strip() for l in out.splitlines() if re.match(r"^\s+[0-9a-f]+:", l)]


loops = [m for m in LOOP.finditer(data) if any("<FcPatternGetString@plt>" in l for l in disasm(m.start(), m.end()))]
if len(loops) != 2 or loops[1].start() - loops[0].start() > 0x400:
    sys.exit(f"release-webkit-patch: expected Skia's 2 family-name loops in {lib}, found {len(loops)}")
for m in loops:
    first = disasm(m.start(), m.start() + 3)
    if not first or "cmp    $0x3,%eax" not in first[0]:     # file offset must be the address objdump shows
        sys.exit(f"release-webkit-patch: {lib}: no cmp $0x3,%eax at file offset {m.start():#x}")
    at = m.start() + 3
    if check:
        if data[at] != 0xeb:
            sys.exit(f"release-webkit-patch: {lib}: the family-name loop at {m.start():#x} is NOT patched")
    else:
        data[at] = 0xeb
if not check:
    open(lib, "wb").write(data)
for m in loops:
    print(f"release-webkit-patch: {'checked' if check else 'patched'} {m.start():#x}: " + " | ".join(disasm(m.start(), m.start() + 5)))
