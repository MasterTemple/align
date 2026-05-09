Input:

let some_var = 3.1;
let another_var = 72.0;

Command:

align =

Output:

let some_var    = 3.1;
let another_var = 72.0;

Command:

align = . -r

Output:

let some_var    =  3.1;
let another_var = 72.0;

Command:

align = '\.' -p 0

Output:

let some_var=3.1; 
let another_var = 72.0; 

---

Command

let some_var = 3.1; # a comment
let another_var = 72.01; # another comment

let some_var    =  3.1; # a comment
let another_var = 72.01; # another comment

let some_var = 3.1; // a comment
let another_var = 72.01; // another comment

let some_var = 3.1; // a comment
let another_var = 72.01; // another comment

hi 3 ok
o 12 no


select *
from first_table
     join some_table T       on T.asdfe = O.sdf30,           -- some comment
left join some_other_table O on O.b43   = T.jf81asfdsafadsf, -- another comment!


select *
from first_table
join some_table T on T.onefield=O.twofield, -- some comment
left join some_other_table O on O.redfield = T.bluefield, -- another comment!

Another test:

When I run `align join on = --`

on

```
join some_table T on T.onefield=O.twofield, -- some comment
left join some_other_table O on O.redfield = T.bluefield, -- another comment!
```

it should result in

```
     join some_table T       on T.onefield = O.twofield,  -- some comment
left join some_other_table O on O.redfield = T.bluefield, -- another comment!
```

but instead it is

```
join     join some_table T on       on T.onefield=  = O.twofield, --  -- some comment
left join join some_other_table O on on O.redfield = = T.bluefield, -- -- another comment!
```

---

Input:

select *
from first_table
join some_table T on T.asdfe=O.sdf30, -- some comment
left join some_other_table O on O.b43 = T.jf81asfdsafadsf, -- another comment!

Command:

align join on = --

Output

select *
from first_table
     join some_table       T on T.asdfe = O.sdf30,           -- some comment
left join some_other_table O on O.b43   = T.jf81asfdsafadsf, -- another comment!

---

All the provided tests pass

Here are some more test cases that don't work

Command align align . -p 0 -c /\d+$/


Interesting bug (maybe not from my progres):
- I must use `'#'` in both Vim and my shell, if I don't wrap in quotes Vim does nothing, but my shell gives me an error telling me the usage
- I have a similar thing when using `;` without quotes

```
let some_var = 1; # a comment
let another_var = 2; # another comment
```

Bug (likely from my program):
- If I try `'/'` or `'//'` to align comments, I get the following

Input:

```
let some_var = 1;    // a comment
let another_var = 2; // another comment
```

Output:

```
  l  e  t  s  o  m  e  _  v  a  r  =  1  ; // a comment
  l  e  t  a  n  o  t  h  e  r  _  v  a  r = 2; // another comment
```

echo "let some_var = 1; # a commentlet another_var = 2; # another comment" | align '#'

---

Thanks for the fix, it now works for `align '//'`, but not for `align '/'`
which outputs

let some_var = 1;    / / a comment
let another_var = 2; / / another comment

ACTUALLY That is correct, because `/` has a default pad of 1

---


