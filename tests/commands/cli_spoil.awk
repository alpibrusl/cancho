# cli_spoil.awk -- ways of spoiling the layout of a program that leave it the same program (the generator of `selfhost::cli::the_command_line_in_cancho_formats_like_cancho_fmt`, in awk and
# with a choice per line that is a function of the line number and length, so that it is the same on every awk): no indentation, CRLF, white space at the ends of lines, blank lines doubled,
# comments added at the ends of lines and between them, everything on one line, lines blanked.   awk -v mode=N -f cli_spoil.awk FILE > SPOILT   (N is 0 to 6)
function trim(s) { sub(/^[ \t]+/, "", s); sub(/[ \t]+$/, "", s); return s }
{
    l = $0
    r = (NR * 37 + length(l) * 11 + 5) % 100
    if (mode == 0) { sub(/^[ \t]+/, "", l); print l }
    else if (mode == 1) { printf "%s\r\n", l }
    else if (mode == 2) { if (trim(l) == "") print l; else print l "  " }
    else if (mode == 3) { print l; print "" }
    else if (mode == 4) {
        if (r < 8) print "    // inserted " (r * 7 % 100)
        if (r > 92 && trim(l) != "" && index(l, "//") == 0) print l " // end " (r * 3 % 100)
        else print l
    }
    else if (mode == 5) {
        t = trim(l)
        if (substr(t, 1, 2) != "//") joined = joined (joined == "" ? "" : " ") t
    }
    else { if ((NR * 53 + length(l) * 7) % 10 == 0) print ""; else print l }
}
END {
    if (mode == 5) { gsub(/;/, ";\n", joined); print joined }
}
