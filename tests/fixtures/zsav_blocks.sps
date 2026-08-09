* Multi-block ZSAV fixture for spss_sav.
* Regenerate with:  pspp zsav_blocks.sps
*
* `compression_zlib.sav` holds a single 232-byte block, so it never
* makes the reader refill. This file exists to cross a block boundary:
* PSPP writes blocks of 0x3ff000 (4 190 208) uncompressed bytes, so the
* command stream has to exceed that to be cut in two.
*
* The shape is chosen to reach that size cheaply and to make the cut
* land somewhere demanding:
*   id     -- 1..2000, so a decoder that loses its place across the
*             refill produces the wrong number and the test sees it
*   filler -- 250 columns of a constant outside the inline range, so
*             every one costs a 253 command plus an 8-byte verbatim
*             payload. That is what makes a row 2258 stream bytes wide
*             rather than 251, and it is what puts the boundary inside
*             a command group's payload run rather than between groups.
*
* Measured on the generated file: the stream is 4 516 792 bytes over two
* blocks, and the boundary at 4 190 208 falls inside row 1856 with 856
* of its 2008 bytes produced, one command into a group whose remaining
* payloads all live in the second block. Both the partly-filled row and
* the partly-consumed group therefore have to survive the refill.
*
* The constant filler is not laziness: it lets deflate crush the bulk of
* the stream, which is what keeps a 4.5 MB stream down to an 84 KB
* fixture. Row identity comes from `id` alone.

INPUT PROGRAM.
NUMERIC id (F8.0).
VECTOR filler (250, F12.0).
LOOP #i = 1 TO 2000.
+ COMPUTE id = #i.
+ LOOP #j = 1 TO 250.
+   COMPUTE filler(#j) = 1000000.
+ END LOOP.
+ END CASE.
END LOOP.
END FILE.
END INPUT PROGRAM.

VARIABLE LABELS id 'Row number, to catch a decoder losing its place'.

SAVE OUTFILE='zsav_blocks.sav' /ZCOMPRESSED.
