package tructiep

// NgauNhien – bộ sinh số giả ngẫu nhiên Mersenne Twister MT19937, khởi tạo và rút số giống hệt
// random.Random(seed) của Python (init_by_array, random() 53 bit, randrange qua getrandbits). Nhờ vậy
// với cùng hạt giống, bản Go chọn đúng những gói "bị mất" như bản Python (--mat-goi) và vẽ đúng các
// hạt nhiễu của nền camera giả lập – kết quả hai bản so sánh được với nhau.
// (math/rand của Go dùng thuật toán khác nên cùng hạt giống vẫn ra dãy khác.)
type NgauNhien struct {
	mt  [624]uint32
	chi int
}

func MoiNgauNhien(hat uint32) *NgauNhien {
	r := &NgauNhien{}
	r.khoiTao(19650218)
	i, j := 1, 0
	for k := 624; k > 0; k-- { // init_by_array với khóa một phần tử [hat]
		r.mt[i] = (r.mt[i] ^ ((r.mt[i-1] ^ (r.mt[i-1] >> 30)) * 1664525)) + hat + uint32(j)
		i++
		j = 0
		if i >= 624 {
			r.mt[0] = r.mt[623]
			i = 1
		}
	}
	for k := 623; k > 0; k-- {
		r.mt[i] = (r.mt[i] ^ ((r.mt[i-1] ^ (r.mt[i-1] >> 30)) * 1566083941)) - uint32(i)
		i++
		if i >= 624 {
			r.mt[0] = r.mt[623]
			i = 1
		}
	}
	r.mt[0] = 0x80000000
	return r
}

func (r *NgauNhien) khoiTao(s uint32) {
	r.mt[0] = s
	for i := 1; i < 624; i++ {
		r.mt[i] = 1812433253*(r.mt[i-1]^(r.mt[i-1]>>30)) + uint32(i)
	}
	r.chi = 624
}

func (r *NgauNhien) uint32() uint32 {
	if r.chi >= 624 {
		for k := 0; k < 624; k++ {
			y := (r.mt[k] & 0x80000000) | (r.mt[(k+1)%624] & 0x7fffffff)
			v := r.mt[(k+397)%624] ^ (y >> 1)
			if y&1 != 0 {
				v ^= 0x9908b0df
			}
			r.mt[k] = v
		}
		r.chi = 0
	}
	y := r.mt[r.chi]
	r.chi++
	y ^= y >> 11
	y ^= (y << 7) & 0x9d2c5680
	y ^= (y << 15) & 0xefc60000
	y ^= y >> 18
	return y
}

// Float64 – như random.random() của Python: số thực 53 bit trong [0, 1).
func (r *NgauNhien) Float64() float64 {
	a, b := r.uint32()>>5, r.uint32()>>6
	return (float64(a)*67108864 + float64(b)) * (1.0 / 9007199254740992)
}

// Intn – như random.randrange(n) của Python (n ≥ 1): rút getrandbits(k) tới khi < n.
func (r *NgauNhien) Intn(n int) int {
	k := 0
	for x := n; x > 0; x >>= 1 {
		k++
	}
	for {
		v := int(r.uint32() >> (32 - k))
		if v < n {
			return v
		}
	}
}
