* Comprehensive spss_sav real-file test fixture generator.
* Regenerate with:  pspp comprehensive.sps
* Exercises: numeric/string/very-long-string vars, value labels,
* long (string) value labels, numeric + long-string missing values,
* variable + file attributes, documents, and multiple response sets
* (subtype 7: C/D groups; subtype 19: E / COUNTEDVALUES group).
* NOTE: the file's creation timestamp and any DOCUMENT "(Entered ...)"
* line depend on generation time; tests must not assert on those.

DATA LIST LIST /id (F2.0) q1 (F1.0) q2 (F1.0) q3 (F1.0) longstr (A300) shortstr (A4).
BEGIN DATA
1 1 0 1 "alpha" "aa"
2 0 1 1 "beta" "bb"
END DATA.

VARIABLE LABELS id 'Identifier' q1 'Question 1' longstr 'A long string variable'.
VALUE LABELS q1 q2 q3 0 'No' 1 'Yes'.
MISSING VALUES id (99).
ADD VALUE LABELS longstr 'alpha' 'First value' 'beta' 'Second value'.
MISSING VALUES longstr ('alpha').

VARIABLE ATTRIBUTE VARIABLES=id ATTRIBUTE=MyAttr('hello world').
DATAFILE ATTRIBUTE ATTRIBUTE=Owner('Alice') Project('Census').

DOCUMENT A documentary line for testing.

MRSETS
  /MDGROUP NAME=$dich LABEL='Dichotomy set' CATEGORYLABELS=VARLABELS VALUE=1 VARIABLES=q1 q2 q3
  /MCGROUP NAME=$cat LABEL='Category set' VARIABLES=q1 q2
  /MDGROUP NAME=$counted CATEGORYLABELS=COUNTEDVALUES VALUE=1 VARIABLES=q2 q3.

SAVE OUTFILE='comprehensive.sav'.
