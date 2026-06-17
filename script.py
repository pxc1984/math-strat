import math

defs_costs = sorted([1] * 14 + [2] * 5 + [4] * 5)
forms_costs = sorted([1] * 25 + [2] * 14 + [3.5] * 10)
proofs_forms_costs = sorted([1] * 15 + [2] * 10 + [3] * 8)
proofs_body_costs = sorted([2] * 10 + [4] * 13 + [7] * 10)

print('Немного подождите')


def get_def_pmf(k_def):
    pmf = {}
    total = math.comb(24, 3)
    for x in range(4):
        if k_def >= x and (24 - k_def) >= (3 - x):
            pmf[x] = math.comb(k_def, x) * math.comb(24 - k_def, 3 - x) / total
    return pmf


def get_form_pmf(k_pf, k_extra, drawn_pf_known):
    pmf = {}
    total_remaining = 45
    known_remaining = (k_pf - drawn_pf_known) + k_extra
    total_ways = math.comb(total_remaining, 2)
    for x in range(3):
        if known_remaining >= x and (total_remaining - known_remaining) >= (2 - x):
            pmf[x] = math.comb(known_remaining, x) * math.comb(total_remaining - known_remaining, 2 - x) / total_ways
    return pmf


def_pmfs = [get_def_pmf(k) for k in range(25)]
form_pmfs_cache = {}
for k_pf in range(34):
    for k_extra in range(50 - k_pf):
        for drawn in range(5):
            form_pmfs_cache[(k_pf, k_extra, drawn)] = get_form_pmf(k_pf, k_extra, drawn)


def get_ticket_pmf(k_def, k_pf, k_pp, k_extra):
    ticket_pmf = {}
    def_pmf = def_pmfs[k_def]
    total_proof_ways = math.comb(33, 4)
    
    for a in range(5):
        for b in range(5 - a):
            c = 4 - a - b
            if k_pp >= a and (k_pf - k_pp) >= b and (33 - k_pf) >= c:
                prob_proof_combo = (math.comb(k_pp, a) * math.comb(k_pf - k_pp, b) * math.comb(33 - k_pf, c)) / total_proof_ways
                if prob_proof_combo <= 0:
                    continue
                
                if a > 0:
                    proof_outcomes = [(3 * a + b + 1, prob_proof_combo * (a / 4)), 
                                      (3 * a + b, prob_proof_combo * (1 - a / 4))]
                else:
                    proof_outcomes = [(b, prob_proof_combo)]
                
                form_pmf = form_pmfs_cache[(k_pf, k_extra, a + b)]
                
                for p_score, p_prob in proof_outcomes:
                    for f_score, f_prob in form_pmf.items():
                        for d_score, d_prob in def_pmf.items():
                            total_score = d_score + f_score + p_score
                            total_prob = p_prob * f_prob * d_prob
                            ticket_pmf[total_score] = ticket_pmf.get(total_score, 0.0) + total_prob
                            
    return ticket_pmf


def check_score(ticket_pmf):
    cum_p = 0
    for s in sorted(ticket_pmf.keys(), reverse=True):
        cum_p += ticket_pmf[s]
        if cum_p >= 0.9:
            return s
    return 0


best = {}

for k_def in range(25):
    c_def = sum(defs_costs[:k_def])
    for k_pf in range(34):
        c_pf = sum(proofs_forms_costs[:k_pf])
        for k_pp in range(k_pf + 1):
            c_pp = sum(proofs_body_costs[:k_pp])
            for k_extra in range(49 - k_pf + 1):
                c_form = sum(forms_costs[:k_extra])
                
                total_cost = c_def + c_pf + c_pp + c_form
                ticket_pmf = get_ticket_pmf(k_def, k_pf, k_pp, k_extra)
                s = check_score(ticket_pmf)
                
                if s not in best or total_cost < best[s][0]:
                    best[s] = (total_cost, k_def, k_extra, k_pf, k_pp)

target = int(input('Сколько хотите баллов (только число): '))

if target in best:
    cost, kd, k_extra, kpf, kpp = best[target]
    print(f'\nПлан на {target} баллов (90%):')
    print(f'Опры: {kd} шт')
    print(f'Формулировки к докам: {kpf} шт')
    print(f'Чистые формулировки: {k_extra} шт')
    print(f'Доки: {kpp} шт')
else:
    print('\nМаксимум 18')