# Brief: `loglens`

> `loglens` reads a log on standard input and prints three lines:
>
> ```
> lines 6
> errors 2
> longest 15
> ```
>
> `lines` is how many lines the log has. `errors` is how many of them
> contain the text `ERROR`. `longest` is the length of the longest line.
>
> **Authority:** it may read the console, write the console, and use the
> heap. Nothing else.

It is split in two on purpose. `main.cho` is given: it reads the log and
prints the report, and it is the only file with authority in it. The
counting is three pure functions the agent writes, and you write the
contract they must meet:

```
pub fn count_lines[&t](text: &t [byte]) -> [] int
pub fn count_matching[&t, &n](text: &t [byte], needle: &n [byte]) -> [] int
pub fn longest_line[&t](text: &t [byte]) -> [] int
```

Notice what the brief does not say. What is a line, when the log does not
end in a newline? What is the length of a line -- does the newline count?
Two `ERROR`s in one line? Those are not details. They are the spec.
