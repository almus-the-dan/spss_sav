* Weight-variable fixture for spss_sav.
* Regenerate with:  pspp weighted.sps
* The A300 variable before the weight is the point: it occupies 38
* physical variable records across 2 segments, so the header's physical
* weight index (40) must be translated through segment (2) to logical
* variable index (2). A reader that conflates those spaces resolves the
* wrong variable, or none.

DATA LIST LIST /id (F2.0) descr (A300) wgt (F4.2).
BEGIN DATA
1 "alpha" 1.5
2 "beta" 2.5
END DATA.

WEIGHT BY wgt.
SAVE OUTFILE='weighted.sav'.
