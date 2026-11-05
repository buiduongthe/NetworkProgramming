import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Locale;
import java.util.Map;

/**
 * Bài mở rộng MR1.1 – Quản lý địa chỉ IP (IPAM).
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp Mục 1.2.3 (địa chỉ IP, tiền tố mạng) và Bài 1.1.
 * Chương trình đọc một kế hoạch địa chỉ (tệp JSON) và:
 *   1. Chia khối địa chỉ thành các mạng con có kích thước khác nhau (VLSM):
 *      yêu cầu lớn được cấp trước để các khối luôn thẳng hàng (aligned).
 *   2. Liệt kê phần địa chỉ còn trống dưới dạng các khối CIDR.
 *   3. Kiểm tra các tiền tố đề xuất: sai cú pháp, có bit máy, nằm ngoài khối, chồng lấn.
 *   4. Cấp phát địa chỉ cho thiết bị trong một mạng con (bỏ qua địa chỉ cổng mặc định).
 * JDK không có lớp "mạng con" (InetAddress chỉ biểu diễn một địa chỉ), nên bài đổi địa chỉ IPv4
 * sang số nguyên 32 bit (kiểu long cho khỏi lo dấu) để tính toán.
 *
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out QuanLyIP [tệp_kế_hoạch.json ...]      (mặc định ke_hoach.json)
 */
public class QuanLyIP {

    /** Một mạng IPv4: địa chỉ mạng (số 32 bit) và độ dài tiền tố. */
    record Net(long addr, int plen) {
        long size() { return 1L << (32 - plen); }
        long last() { return addr + size() - 1; }                    // địa chỉ quảng bá
        long mask() { return (0xFFFFFFFFL << (32 - plen)) & 0xFFFFFFFFL; }
        boolean overlaps(Net o) { return addr <= o.last() && o.addr <= last(); }
        boolean subnetOf(Net o) { return addr >= o.addr && last() <= o.last(); }
        @Override public String toString() { return ipStr(addr) + "/" + plen; }
    }

    /** Một mạng con đã cấp: tên, số máy cần, mạng. */
    record Alloc(String name, int need, Net net) {}

    static String ipStr(long v) {
        return ((v >> 24) & 255) + "." + ((v >> 16) & 255) + "." + ((v >> 8) & 255) + "." + (v & 255);
    }

    /** Đọc "a.b.c.d" thành số 32 bit; ném IllegalArgumentException nếu sai cú pháp. */
    static long parseIp(String s) {
        String[] p = s.split("\\.", -1);
        if (p.length != 4) throw new IllegalArgumentException("cần 4 phần");
        long v = 0;
        for (String x : p) {
            if (x.isEmpty() || x.length() > 3 || !x.chars().allMatch(Character::isDigit))
                throw new IllegalArgumentException("phần không hợp lệ: " + x);
            int b = Integer.parseInt(x);
            if (b > 255) throw new IllegalArgumentException("phần > 255: " + x);
            v = (v << 8) | b;
        }
        return v;
    }

    /**
     * Đọc "a.b.c.d/n". strict = true: ném lỗi nếu còn bit máy (giống ip_network(strict=True) của Python);
     * strict = false: xóa bit máy (chuẩn hóa về địa chỉ mạng).
     */
    static Net parseNet(String text, boolean strict) {
        int slash = text.indexOf('/');
        if (slash < 0) throw new IllegalArgumentException("thiếu /tiền tố");
        long ip = parseIp(text.substring(0, slash));
        String ps = text.substring(slash + 1);
        if (ps.isEmpty() || !ps.chars().allMatch(Character::isDigit) || ps.length() > 2)
            throw new IllegalArgumentException("tiền tố không hợp lệ");
        int plen = Integer.parseInt(ps);
        if (plen > 32) throw new IllegalArgumentException("tiền tố > 32");
        long mask = plen == 0 ? 0 : (0xFFFFFFFFL << (32 - plen)) & 0xFFFFFFFFL;
        if (strict && (ip & ~mask & 0xFFFFFFFFL) != 0) throw new IllegalArgumentException("có bit máy");
        return new Net(ip & mask, plen);
    }

    /** Khối riêng theo RFC 1918: 10/8, 172.16/12, 192.168/16. */
    static boolean isPrivate(Net n) {
        Net[] priv = {parseNet("10.0.0.0/8", true), parseNet("172.16.0.0/12", true), parseNet("192.168.0.0/16", true)};
        for (Net p : priv) if (n.subnetOf(p)) return true;
        return false;
    }

    /** Độ dài tiền tố nhỏ nhất chứa được nHosts máy (+2 địa chỉ mạng và quảng bá). */
    static int prefixForHosts(int nHosts) {
        long size = 4;                                   // khối nhỏ nhất dùng ở đây là /30
        while (size - 2 < nHosts) size *= 2;
        return 32 - Long.numberOfTrailingZeros(size);
    }

    /** Cấp mạng con cho từng yêu cầu (lớn trước nhỏ sau); ném IllegalStateException nếu hết chỗ. */
    static List<Alloc> planVlsm(Net block, List<Map<String, Object>> requests) {
        List<Map<String, Object>> order = new ArrayList<>(requests);
        order.sort(Comparator.comparingLong((Map<String, Object> r) -> (Long) r.get("so_may")).reversed()); // sắp xếp ổn định
        long cursor = block.addr(), end = block.last();
        List<Alloc> result = new ArrayList<>();
        for (Map<String, Object> r : order) {
            String name = (String) r.get("ten");
            int need = ((Long) r.get("so_may")).intValue();
            int plen = prefixForHosts(need);
            long size = 1L << (32 - plen);
            cursor = (cursor + size - 1) / size * size;            // làm tròn lên bội của kích thước khối
            if (cursor + size - 1 > end)
                throw new IllegalStateException("Không đủ chỗ cho " + name + " (" + need + " máy cần /" + plen
                        + ") trong " + block);
            result.add(new Alloc(name, need, new Net(cursor, plen)));
            cursor += size;
        }
        return result;
    }

    /** Tách dải [lo, hi] thành ít khối CIDR nhất (mỗi khối thẳng hàng, lớn nhất có thể). */
    static List<Net> rangeToCidr(long lo, long hi) {
        List<Net> out = new ArrayList<>();
        while (lo <= hi) {
            int plen = 32;
            while (plen > 0) {
                long size = 1L << (32 - (plen - 1));
                if (lo % size != 0 || lo + size - 1 > hi) break;     // khối lớn hơn không thẳng hàng hoặc vượt dải
                plen--;
            }
            Net n = new Net(lo, plen);
            out.add(n);
            lo = n.last() + 1;
        }
        return out;
    }

    /** Các khối CIDR còn trống trong block sau khi trừ đi các mạng đã dùng. */
    static List<Net> freeBlocks(Net block, List<Net> used) {
        List<Net> sorted = new ArrayList<>(used);
        sorted.sort(Comparator.comparingLong(Net::addr));
        List<Net> free = new ArrayList<>();
        long cursor = block.addr();
        for (Net u : sorted) {
            if (u.addr() > cursor) free.addAll(rangeToCidr(cursor, u.addr() - 1));
            cursor = Math.max(cursor, u.last() + 1);
        }
        if (cursor <= block.last()) free.addAll(rangeToCidr(cursor, block.last()));
        return free;
    }

    /** Kiểm tra một tiền tố đề xuất, trả về chuỗi kết luận. */
    static String checkPrefix(String text, Net block, List<Alloc> allocated) {
        Net net;
        String note = "";
        try {
            net = parseNet(text, true);
        } catch (IllegalArgumentException e) {
            try {
                net = parseNet(text, false);                        // thử lại, chấp nhận bit máy
                note = "có bit máy, chuẩn hóa " + net + "; ";
            } catch (IllegalArgumentException e2) {
                return "KHÔNG HỢP LỆ (sai cú pháp địa chỉ hoặc tiền tố)";
            }
        }
        if (!net.subnetOf(block)) return note + "NGOÀI KHỐI " + block;
        for (Alloc a : allocated)
            if (net.overlaps(a.net())) return note + "CHỒNG LẤN với " + a.name() + " (" + a.net() + ")";
        return note + "HỢP LỆ, còn trống";
    }

    /** Địa chỉ khả dụng đầu tiên làm cổng mặc định; các thiết bị nhận các địa chỉ tiếp theo. */
    static List<String[]> assignHosts(Net net, List<Object> devices) {
        List<String[]> result = new ArrayList<>();
        long next = net.addr() + 1;                                 // bỏ địa chỉ mạng
        result.add(new String[] {"gateway", ipStr(next++)});
        for (Object d : devices) {
            if (next >= net.last())                                 // không cấp địa chỉ quảng bá
                throw new IllegalStateException("Mạng " + net + " đã hết địa chỉ khi cấp cho " + d);
            result.add(new String[] {(String) d, ipStr(next++)});
        }
        return result;
    }

    /** Xử lý một tệp kế hoạch; trả về false nếu kế hoạch không thực hiện được. */
    @SuppressWarnings("unchecked")
    static boolean runPlan(String path) throws Exception {
        Map<String, Object> plan = (Map<String, Object>) Json.parse(Files.readString(Path.of(path), StandardCharsets.UTF_8));
        System.out.println("===== Kế hoạch: " + path + " =====");
        Net block = parseNet((String) plan.get("khoi_dia_chi"), true);
        System.out.println("[ipam] Khối địa chỉ " + block + ": " + block.size() + " địa chỉ, "
                + (isPrivate(block) ? "riêng (RFC 1918)" : "công cộng"));
        List<Alloc> allocated;
        try {
            List<Map<String, Object>> req = new ArrayList<>();
            for (Object o : (List<Object>) plan.get("yeu_cau")) req.add((Map<String, Object>) o);
            allocated = planVlsm(block, req);
        } catch (IllegalStateException e) {
            System.out.println("[ipam] LỖI: " + e.getMessage());
            return false;
        }

        System.out.println("\n[ipam] 1) Chia mạng con (VLSM, yêu cầu lớn cấp trước)");
        System.out.printf("  %-14s %4s %8s %-16s %-15s Dải khả dụng%n", "Tên", "Cần", "Khả dụng", "Mạng con", "Mặt nạ");
        long used = 0;
        for (Alloc a : allocated) {
            Net n = a.net();
            System.out.printf("  %-14s %4d %8d %-16s %-15s %s - %s%n", a.name(), a.need(), n.size() - 2, n,
                    ipStr(n.mask()), ipStr(n.addr() + 1), ipStr(n.last() - 1));  // bỏ địa chỉ mạng, quảng bá
            used += n.size();
        }
        System.out.println("  Đã dùng " + used + "/" + block.size() + " địa chỉ ("
                + String.format(Locale.ROOT, "%.1f", used * 100.0 / block.size()) + "%)");

        System.out.println("\n[ipam] 2) Khối còn trống");
        List<Net> nets = new ArrayList<>();
        for (Alloc a : allocated) nets.add(a.net());
        for (Net f : freeBlocks(block, nets))
            System.out.printf("  %-17s (%d địa chỉ)%n", f, f.size());

        List<Object> checks = (List<Object>) plan.get("kiem_tra");
        if (checks != null && !checks.isEmpty()) {
            System.out.println("\n[ipam] 3) Kiểm tra tiền tố đề xuất");
            for (Object t : checks)
                System.out.printf("  %-17s -> %s%n", t, checkPrefix((String) t, block, allocated));
        }

        Map<String, Object> req = (Map<String, Object>) plan.get("cap_phat");
        if (req != null) {
            Net net = allocated.stream().filter(a -> a.name().equals(req.get("mang"))).findFirst().orElseThrow().net();
            System.out.println("\n[ipam] 4) Cấp phát địa chỉ trong " + req.get("mang") + " (" + net + ")");
            for (String[] r : assignHosts(net, (List<Object>) req.get("thiet_bi")))
                System.out.printf("  %-8s %s/%d%n", r[0], r[1], net.plen());
        }
        return true;
    }

    public static void main(String[] args) throws Exception {
        String[] paths = args.length > 0 ? args : new String[] {"ke_hoach.json"};
        boolean ok = true;
        for (String p : paths) ok &= runPlan(p);
        System.exit(ok ? 0 : 1);
    }
}
