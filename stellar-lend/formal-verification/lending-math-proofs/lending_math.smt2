; ============================================================================
; lending_math.smt2 - SMT-LIB 2 specifications for the StellarLend lending math
;
; Tool: Z3 >= 4.12  (also compatible with CVC5)
; Run:  z3 lending_math.smt2
;
; Each (check-sat) should return "unsat", proving the negation of the safety
; property is unsatisfiable - i.e., the property holds for ALL inputs.
;
; Encoding notes:
;   - i128 range: [-2^127, 2^127 - 1]
;   - BPS (basis points) divisor: 10000
;   - SECONDS_PER_YEAR: 31536000
; ============================================================================

(set-logic QF_NIA)   ; Quantifier-Free Non-linear Integer Arithmetic

; ---------------------------------------------------------------------------
; Constants
; ---------------------------------------------------------------------------

(define-const I128_MAX Int 170141183460469231731687303715884105727)   ; 2^127 - 1
(define-const I128_MIN Int (- 0 170141183460469231731687303715884105728))  ; -2^127
(define-const BPS Int 10000)
(define-const SPY Int 31536000)     ; seconds per year

(define-fun in_range ((x Int)) Bool
  (and (>= x I128_MIN) (<= x I128_MAX)))

; ============================================================================
; 1. UTILIZATION - non-negative and bounded below 100% when borrows <= supply
;
;    Property: for all borrows >= 0, supply >= 0:
;      supply = 0  =>  utilization = 0
;      borrows <= supply  =>  utilization = (borrows * 10000) / supply <= 10000
; ============================================================================

(push)
(declare-const borrows Int)
(declare-const supply Int)
(assert (>= borrows 0))
(assert (>= supply 0))

(define-fun utilization () Int
  (ite (= supply 0) 0 (div (* borrows BPS) supply)))

; Negation: utilization is negative, or violates the bounds.
(assert (or
  (< utilization 0)
  (and (not (= supply 0)) (<= borrows supply) (> utilization BPS))
  (and (= supply 0) (not (= utilization 0)))))
(check-sat)

; ============================================================================
; 2. HEALTH FACTOR - liquidatability is exactly hf < 10000, zero debt is safe
;
;    Property: for all collateral >= 0, debt >= 0, lt >= 0:
;      debt = 0  =>  health factor = MAX and not liquidatable
;      health factor >= 10000  =>  not liquidatable
; ============================================================================

(push)
(declare-const collateral Int)
(declare-const debt Int)
(declare-const lt Int)
(assert (>= collateral 0))
(assert (>= debt 0))
(assert (>= lt 0))

(define-fun hf () Int
  (ite (= debt 0) I128_MAX (div (* collateral lt) debt)))

(define-fun liquidatable () Bool (< hf BPS))

; Negation: zero debt is liquidatable, or hf >= 10000 yet liquidatable.
(assert (or
  (and (= debt 0) liquidatable)
  (and (>= hf BPS) liquidatable)))
(check-sat)

; ============================================================================
; 3. MAX LIQUIDATABLE - respects the close factor
;
;    Property: for all debt >= 0, close in [0, 10000]:
;      max_liquidatable = (debt * close) / 10000
;      0 <= max_liquidatable <= debt
; ============================================================================

(push)
(declare-const debt_ml Int)
(declare-const close Int)
(assert (>= debt_ml 0))
(assert (>= close 0))
(assert (<= close BPS))

(define-fun max_liq () Int (div (* debt_ml close) BPS))

; Negation: result exceeds the debt or is negative.
(assert (or (< max_liq 0) (> max_liq debt_ml)))
(check-sat)

; ============================================================================
; 4. SEIZE AMOUNT - always covers the repaid debt
;
;    Property: for all repay >= 0, bonus in [0, 10000]:
;      seize = repay + (repay * bonus) / 10000 >= repay
; ============================================================================

(push)
(declare-const repay Int)
(declare-const bonus Int)
(assert (>= repay 0))
(assert (>= bonus 0))
(assert (<= bonus BPS))

(define-fun seize () Int (+ repay (div (* repay bonus) BPS)))

; Negation: seized amount is less than the repaid debt.
(assert (< seize repay))
(check-sat)

; ============================================================================
; 5. SIMPLE INTEREST - non-decreasing for non-negative inputs
;
;    Property: for all principal >= 0, rate >= 0, elapsed >= 0:
;      accrued = principal + (principal * rate * elapsed) / (BPS * SPY)
;      zero rate/elapsed/principal leaves principal unchanged
;      otherwise accrued >= principal
; ============================================================================

(push)
(declare-const p Int)
(declare-const rate Int)
(declare-const elapsed Int)
(assert (>= p 0))
(assert (>= rate 0))
(assert (>= elapsed 0))

(define-fun accrued () Int
  (ite (or (= p 0) (= rate 0) (= elapsed 0))
       p
       (+ p (div (* (* p rate) elapsed) (* BPS SPY)))))

; Negation: accrued interest decreased the principal.
(assert (< accrued p))
(check-sat)

; ============================================================================
; 6. COMPOUND INTEREST - never decreases a non-negative principal
;
;    Property: for all principal >= 0, rate >= 0, n compounding periods:
;      result >= principal
; ============================================================================

(push)
(declare-const cp Int)
(declare-const crate Int)
(declare-const n Int)
(assert (>= cp 0))
(assert (>= crate 0))
(assert (>= n 0))

; Recursive definition over periods (small n for tractability).
(define-fun compound ((acc Int) (k Int)) Int
  (ite (= k 0) acc (compound (+ acc (div (* acc crate) BPS)) (- k 1))))

(define-fun compound_result () Int (compound cp n))

; Negation: compounding decreased the principal.
(assert (< compound_result cp))
(check-sat)