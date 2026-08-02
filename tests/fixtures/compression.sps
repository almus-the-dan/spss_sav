* Compression test fixtures for spss_sav.
* Regenerate with:  pspp compression.sps
*
* Writes the SAME data three ways so the record reader can be checked
* against a single expected row set across every compression scheme:
*   compression_none.sav   -- code 0, uncompressed
*   compression_bytecode.sav -- code 1, bytecode
*   compression_zlib.sav   -- code 2, ZSAV
*
* The data is chosen to exercise every bytecode command byte:
*   small   -- integers in 1..251 minus the bias, stored inline (codes 1-251)
*   big     -- outside the inline range, so a verbatim element follows (253)
*   frac    -- non-integral, also verbatim (253)
*   sysmis  -- system missing (255)
*   blank   -- an A8 whose eight bytes are all spaces (254)
*   text    -- an ordinary short string, verbatim (253)
*   longstr -- an A300 very long string, two segments
* NOTE: the creation timestamp depends on generation time; tests must
* not assert on it.

DATA LIST LIST /id (F2.0) small (F3.0) big (F12.0) frac (F8.2) sysmis (F3.0)
                blank (A8) text (A4) longstr (A300).
BEGIN DATA
1 0    1000000 1.50 . "        " "aa" "alpha"
2 151  -999999 -0.25 . " " "bb" "beta"
3 -100 0       0.00 . "xy" "cc" "gamma"
END DATA.

VARIABLE LABELS id 'Identifier' longstr 'A long string variable'.

* User-defined missing values, one of each shape the format allows, so
* the record reader can be checked on all of them:
*   small   -- a discrete numeric value (row 2)
*   big     -- an open-ended range, LOWEST THRU 0 (rows 2 and 3)
*   text    -- a short-string value (row 3)
*   longstr -- a very-long-string value, which PSPP writes to extension
*              subtype 22 keyed by the LONG name and truncated to eight
*              bytes (row 2)
MISSING VALUES small (151).
MISSING VALUES big (LOWEST THRU 0).
MISSING VALUES text ('cc').
MISSING VALUES longstr ('beta').

SAVE OUTFILE='compression_none.sav' /UNCOMPRESSED.
SAVE OUTFILE='compression_bytecode.sav' /COMPRESSED.
SAVE OUTFILE='compression_zlib.sav' /ZCOMPRESSED.
