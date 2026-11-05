/** Tiện ích địa chỉ IPv4 dùng chung cho thiết bị mô phỏng và chương trình tự động hóa (thay cho ipaddress của Python). */
final class DiaChi {
    private DiaChi() {}

    /** Đọc "a.b.c.d" thành số 32 bit (long); ném IllegalArgumentException nếu sai. */
    static long parseIp(String s) {
        String[] p = s.split("\\.", -1);
        if (p.length != 4) throw new IllegalArgumentException("địa chỉ sai: " + s);
        long v = 0;
        for (String x : p) {
            if (x.isEmpty() || x.length() > 3 || !x.chars().allMatch(Character::isDigit))
                throw new IllegalArgumentException("địa chỉ sai: " + s);
            int b = Integer.parseInt(x);
            if (b > 255) throw new IllegalArgumentException("địa chỉ sai: " + s);
            v = (v << 8) | b;
        }
        return v;
    }

    static String ipStr(long v) {
        return ((v >> 24) & 255) + "." + ((v >> 16) & 255) + "." + ((v >> 8) & 255) + "." + (v & 255);
    }

    /** Mặt nạ dạng chấm (255.255.255.0) -> độ dài tiền tố; mặt nạ phải liên tục các bit 1. */
    static int maskToPrefix(String mask) {
        long m = parseIp(mask);
        int plen = Long.bitCount(m);
        if (m != prefixToMask(plen)) throw new IllegalArgumentException("mặt nạ không hợp lệ: " + mask);
        return plen;
    }

    static long prefixToMask(int plen) {
        return plen == 0 ? 0 : (0xFFFFFFFFL << (32 - plen)) & 0xFFFFFFFFL;
    }

    /** "ip/độ_dài" -> {ip, mặt nạ dạng chấm}. */
    static String[] splitInterface(String cidr) {
        String[] p = cidr.split("/");
        int plen = Integer.parseInt(p[1]);
        if (plen < 0 || plen > 32) throw new IllegalArgumentException("tiền tố sai: " + cidr);
        return new String[] {ipStr(parseIp(p[0])), ipStr(prefixToMask(plen))};
    }
}
