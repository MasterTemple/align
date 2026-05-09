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

let some_var = 3.1; # a comment
let another_var = 72.01; # another comment

let some_var = 3.1; # a comment
let another_var = 72.01; # another comment

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
