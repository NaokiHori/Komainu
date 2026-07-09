# Mistakes that Komainu reports

Every problem below is reported (with file, line and column) and no output is written.
Each case is a separate placeholder; the line before it says what is wrong.

An undefined name (a typo):

{{ velocty }}

Several undefined names in one expression are all reported:

{{ density * foo + bar }}

A missing operand:

{{ density + }}

A missing operator; multiplication has to be written with `*`:

{{ 2 density }}

Unbalanced parentheses:

{{ (density + pressure }}

{{ density) }}

Two relations in one expression:

{{ density = pressure = 0 }}

A wrong number of arguments (the operator takes two):

{{ partial(velocity) }}

An operator used without a call, a value used as an operator, and an unknown operator:

{{ partial }}

{{ density(time) }}

{{ unknown(density) }}

A malformed argument list:

{{ partial(velocity time) }}

{{ partial(velocity, ) }}

Raw LaTeX in an expression (define a symbol instead):

{{ \alpha }}

An empty placeholder:

{{ }}

White space is required just inside the braces:

{{ density}}

A placeholder that is never closed swallows the rest of the file, so it has to be last:

{{ density
