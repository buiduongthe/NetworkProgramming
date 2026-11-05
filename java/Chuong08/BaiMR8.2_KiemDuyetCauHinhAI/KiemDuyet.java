import java.io.IOException;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Duration;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Bài mở rộng MR8.2 – Kiểm duyệt cấu hình do LLM sinh ra trước khi dùng (guardrail + vòng phản hồi).
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp Bài 8.1 (gọi API LLM), Bài 3.3 (dữ liệu JSON) và Bài MR1.1
 * (kiểm tra địa chỉ IP). Nguyên tắc: đầu ra của LLM là DỮ LIỆU CHƯA TIN CẬY, giống dữ liệu nhận từ mạng;
 * phải kiểm tra như kiểm tra gói tin từ client lạ:
 *  1. Tách và phân tích JSON (LLM hay bọc JSON trong khối ```json kèm lời dẫn).
 *  2. Lược đồ: đủ trường, đúng kiểu.
 *  3. Ngữ nghĩa: VLAN 1–4094, không trùng; mạng con hợp lệ, nằm trong khối địa chỉ, không chồng lấn nhau và
 *     không đụng hạ tầng đang dùng; cổng mặc định là địa chỉ máy hợp lệ trong mạng.
 *  4. Chính sách an toàn: cấm "permit ip any any", cấm bật telnet.
 * Nếu có lỗi: gửi danh sách lỗi lại cho LLM để sửa (tối đa --so-lan lần); hết lượt vẫn sai thì TỪ CHỐI.
 * Cấu hình đạt mới được chuyển thành lệnh CLI – và vẫn cần người duyệt trước khi áp dụng.
 * Cấu hình qua biến môi trường: LLM_BASE_URL, LLM_API_KEY, LLM_MODEL (như Bài 8.1).
 *
 * Chạy: java -cp out KiemDuyet <ha_tang.json> "<yêu cầu>" [--so-lan N]
 */
public class KiemDuyet {
    static final String BASE_URL = System.getenv().getOrDefault("LLM_BASE_URL", "http://127.0.0.1:9000/v1").replaceAll("/+$", "");
    static final String MODEL = System.getenv().getOrDefault("LLM_MODEL", "mock-model");
    static final String SYSTEM = "Bạn sinh cấu hình mạng. Chỉ trả về một đối tượng JSON có các khóa: "
            + "vlan (danh sách {id, ten, mang, cong_mac_dinh}), acl (danh sách {ten, quy_tac}), dich_vu ({telnet, ssh}).";
    static final Pattern FORBIDDEN_RULES = Pattern.compile("permit\\s+(ip\\s+)?any\\s+any");
    static final Pattern FENCE = Pattern.compile("```(?:json)?\\s*\\n(.*?)```", Pattern.DOTALL);

    // ---------- địa chỉ IPv4 (thay cho mô-đun ipaddress của Python) ----------
    static long parseIp(String s) {
        String[] p = s.split("\\.", -1);
        if (p.length != 4) throw new IllegalArgumentException();
        long v = 0;
        for (String x : p) {
            if (x.isEmpty() || x.length() > 3 || !x.chars().allMatch(Character::isDigit) || Integer.parseInt(x) > 255)
                throw new IllegalArgumentException();
            v = (v << 8) | Integer.parseInt(x);
        }
        return v;
    }

    static String ipStr(long v) { return ((v >> 24) & 255) + "." + ((v >> 16) & 255) + "." + ((v >> 8) & 255) + "." + (v & 255); }

    /** Mạng IPv4: địa chỉ mạng và độ dài tiền tố. */
    record Net(long addr, int plen) {
        long last() { return addr + (1L << (32 - plen)) - 1; }
        long mask() { return plen == 0 ? 0 : (0xFFFFFFFFL << (32 - plen)) & 0xFFFFFFFFL; }
        boolean overlaps(Net o) { return addr <= o.last() && o.addr <= last(); }
        boolean subnetOf(Net o) { return addr >= o.addr && last() <= o.last(); }
        boolean contains(long ip) { return ip >= addr && ip <= last(); }
        @Override public String toString() { return ipStr(addr) + "/" + plen; }
    }

    /** "a.b.c.d/n" chặt chẽ (không chấp nhận bit máy), như ipaddress.ip_network của Python; thiếu "/n" = /32. */
    static Net parseNet(String text) {
        String[] p = text.split("/", -1);
        if (p.length > 2) throw new IllegalArgumentException();
        long ip = parseIp(p[0]);
        int plen = 32;
        if (p.length == 2) {
            if (!p[1].matches("\\d{1,2}") || Integer.parseInt(p[1]) > 32) throw new IllegalArgumentException();
            plen = Integer.parseInt(p[1]);
        }
        Net n = new Net(ip & (plen == 0 ? 0 : (0xFFFFFFFFL << (32 - plen)) & 0xFFFFFFFFL), plen);
        if (n.addr() != ip) throw new IllegalArgumentException();   // có bit máy
        return n;
    }

    // ---------- gọi LLM ----------
    static final HttpClient HTTP = HttpClient.newBuilder().version(HttpClient.Version.HTTP_1_1).connectTimeout(Duration.ofSeconds(10)).build();

    @SuppressWarnings("unchecked")
    static String chat(List<Object> messages) throws IOException, InterruptedException {
        String key = System.getenv("LLM_API_KEY");
        if (key == null || key.isEmpty()) {
            System.err.println("Thiếu biến môi trường LLM_API_KEY");
            System.exit(1);
        }
        Map<String, Object> req = Json.obj("model", MODEL, "messages", messages, "temperature", 0,
                "response_format", Json.obj("type", "json_object"));
        HttpRequest rq = HttpRequest.newBuilder(URI.create(BASE_URL + "/chat/completions")).timeout(Duration.ofSeconds(10))
                .header("Content-Type", "application/json").header("Authorization", "Bearer " + key)
                .POST(HttpRequest.BodyPublishers.ofString(Json.stringify(req), StandardCharsets.UTF_8)).build();
        HttpResponse<String> resp = HTTP.send(rq, HttpResponse.BodyHandlers.ofString(StandardCharsets.UTF_8));
        if (resp.statusCode() != 200) throw new IOException("HTTP " + resp.statusCode());
        Map<String, Object> body = (Map<String, Object>) Json.parse(resp.body());
        Map<String, Object> msg = (Map<String, Object>) ((Map<String, Object>) ((List<Object>) body.get("choices")).get(0)).get("message");
        return (String) msg.get("content");
    }

    /** Lấy JSON từ khối ```json ... ``` nếu có, nếu không thì dùng toàn bộ văn bản. */
    static Object extractJson(String text) {
        Matcher m = FENCE.matcher(text);
        return Json.parse(m.find() ? m.group(1) : text);
    }

    // ---------- các lớp kiểm tra ----------
    @SuppressWarnings("unchecked")
    static List<String> checkSchema(Object cfgObj) {
        List<String> errs = new ArrayList<>();
        if (!(cfgObj instanceof Map<?, ?>)) return List.of("gốc phải là đối tượng JSON");
        Map<String, Object> cfg = (Map<String, Object>) cfgObj;
        if (!(cfg.get("vlan") instanceof List<?>)) errs.add("thiếu khóa vlan hoặc sai kiểu");
        if (!(cfg.get("acl") instanceof List<?>)) errs.add("thiếu khóa acl hoặc sai kiểu");
        if (!(cfg.get("dich_vu") instanceof Map<?, ?>)) errs.add("thiếu khóa dich_vu hoặc sai kiểu");
        if (cfg.get("vlan") instanceof List<?> vl)
            for (int i = 0; i < vl.size(); i++) {
                Map<String, Object> v = vl.get(i) instanceof Map<?, ?> mm ? (Map<String, Object>) mm : Map.of();
                if (!(v.get("id") instanceof Long)) errs.add("vlan[" + i + "].id thiếu hoặc sai kiểu");
                for (String k : new String[] {"ten", "mang", "cong_mac_dinh"})
                    if (!(v.get(k) instanceof String)) errs.add("vlan[" + i + "]." + k + " thiếu hoặc sai kiểu");
            }
        return errs;
    }

    @SuppressWarnings("unchecked")
    static List<String> checkSemantics(Map<String, Object> cfg, Map<String, Object> infra) {
        List<String> errs = new ArrayList<>();
        List<Object[]> nets = new ArrayList<>();                    // {tên, Net}
        Net block = parseNet((String) infra.get("khoi_dia_chi"));
        Map<Long, String> usedIds = new HashMap<>();
        List<Map<String, Object>> daDung = new ArrayList<>();
        for (Object o : (List<Object>) infra.get("da_dung")) {
            Map<String, Object> d = (Map<String, Object>) o;
            daDung.add(d);
            usedIds.put((Long) d.get("vlan"), (String) d.get("ten"));
        }
        Set<Long> seenIds = new HashSet<>();
        for (Object o : (List<Object>) cfg.get("vlan")) {
            Map<String, Object> v = (Map<String, Object>) o;
            String name = (String) v.get("ten");
            long id = (Long) v.get("id");
            if (id < 1 || id > 4094) errs.add(name + ": VLAN " + id + " ngoài miền 1-4094");
            else if (usedIds.containsKey(id)) errs.add(name + ": VLAN " + id + " đã dùng cho " + usedIds.get(id));
            else if (seenIds.contains(id)) errs.add(name + ": VLAN " + id + " bị trùng");
            seenIds.add(id);
            Net net;
            try {
                net = parseNet((String) v.get("mang"));           // chặt chẽ: không chấp nhận bit máy
            } catch (IllegalArgumentException e) {
                errs.add(name + ": mạng " + v.get("mang") + " không hợp lệ");
                continue;
            }
            if (!net.subnetOf(block)) errs.add(name + ": mạng " + net + " ngoài khối " + block);
            try {
                long gw = parseIp((String) v.get("cong_mac_dinh"));
                if (!net.contains(gw) || gw == net.addr() || gw == net.last())
                    errs.add(name + ": cổng mặc định " + ipStr(gw) + " không phải địa chỉ máy của " + net);
            } catch (IllegalArgumentException e) {
                errs.add(name + ": cổng mặc định " + v.get("cong_mac_dinh") + " không phải IPv4 hợp lệ");
            }
            for (Object[] other : nets)
                if (net.overlaps((Net) other[1])) errs.add(name + ": mạng " + net + " chồng lấn " + other[0] + " (" + other[1] + ")");
            for (Map<String, Object> d : daDung)
                if (net.overlaps(parseNet((String) d.get("mang")))) errs.add(name + ": mạng " + net + " đụng hạ tầng " + d.get("ten") + " (" + d.get("mang") + ")");
            nets.add(new Object[] {name, net});
        }
        return errs;
    }

    @SuppressWarnings("unchecked")
    static List<String> checkPolicy(Map<String, Object> cfg) {
        List<String> errs = new ArrayList<>();
        for (Object o : (List<Object>) cfg.get("acl")) {
            Map<String, Object> acl = (Map<String, Object>) o;
            for (Object rule : (List<Object>) acl.getOrDefault("quy_tac", List.of()))
                if (FORBIDDEN_RULES.matcher(String.valueOf(rule)).find())
                    errs.add("ACL " + acl.get("ten") + ": cấm quy tắc \"" + rule + "\"");
        }
        if (!Boolean.FALSE.equals(((Map<String, Object>) cfg.get("dich_vu")).get("telnet")))
            errs.add("dich_vu.telnet phải tắt (mật khẩu đi dạng rõ), dùng SSH");
        return errs;
    }

    record Ket(Map<String, Object> cfg, List<String> errs) {}

    @SuppressWarnings("unchecked")
    static Ket validate(String text, Map<String, Object> infra) {
        Object cfg;
        try {
            cfg = extractJson(text);
        } catch (RuntimeException e) {
            return new Ket(null, List.of("JSON hỏng: " + e.getMessage()));
        }
        List<String> errs = checkSchema(cfg);
        if (!errs.isEmpty()) return new Ket(null, errs);
        List<String> all = new ArrayList<>(checkSemantics((Map<String, Object>) cfg, infra));
        all.addAll(checkPolicy((Map<String, Object>) cfg));
        return new Ket((Map<String, Object>) cfg, all);
    }

    /** Chuyển cấu hình đã duyệt thành lệnh CLI (kiểu IOS) – mẫu đơn giản. */
    @SuppressWarnings("unchecked")
    static List<String> render(Map<String, Object> cfg) {
        List<String> lines = new ArrayList<>();
        for (Object o : (List<Object>) cfg.get("vlan")) {
            Map<String, Object> v = (Map<String, Object>) o;
            Net net = parseNet((String) v.get("mang"));
            lines.addAll(List.of("vlan " + v.get("id"), " name " + v.get("ten"), "interface Vlan" + v.get("id"),
                    " ip address " + v.get("cong_mac_dinh") + " " + ipStr(net.mask())));
        }
        for (Object o : (List<Object>) cfg.get("acl")) {
            Map<String, Object> acl = (Map<String, Object>) o;
            lines.add("ip access-list extended " + acl.get("ten"));
            for (Object r : (List<Object>) acl.get("quy_tac")) lines.add(" " + r);
        }
        lines.add(Boolean.TRUE.equals(((Map<String, Object>) cfg.get("dich_vu")).get("telnet")) ? "service telnet" : "no service telnet");
        return lines;
    }

    @SuppressWarnings("unchecked")
    public static void main(String[] args) throws Exception {
        if (args.length < 2) {
            System.err.println("Cách dùng: java -cp out KiemDuyet <ha_tang.json> \"<yêu cầu>\" [--so-lan N]");
            System.exit(2);
        }
        int soLan = 3;
        for (int i = 2; i + 1 < args.length; i += 2) if (args[i].equals("--so-lan")) soLan = Integer.parseInt(args[i + 1]);
        Map<String, Object> infra = (Map<String, Object>) Json.parse(Files.readString(Path.of(args[0]), StandardCharsets.UTF_8));
        List<Object> messages = new ArrayList<>(List.of(Json.obj("role", "system", "content", SYSTEM), Json.obj("role", "user", "content", args[1])));
        System.out.println("[kiem-duyet] Yêu cầu: " + args[1]);
        for (int attempt = 1; attempt <= soLan; attempt++) {
            String text;
            try {
                text = chat(messages);
            } catch (IOException e) {
                System.out.println("[kiem-duyet] Lỗi gọi LLM: " + e);
                System.exit(1);
                return;
            }
            messages.add(Json.obj("role", "assistant", "content", text));
            Ket k = validate(text, infra);
            if (k.errs().isEmpty()) {
                System.out.println("[kiem-duyet] Lần " + attempt + ": ĐẠT");
                System.out.println("[kiem-duyet] Lệnh CLI sinh từ cấu hình đã duyệt (chờ người duyệt):");
                for (String line : render(k.cfg())) System.out.println("  " + line);
                return;
            }
            System.out.println("[kiem-duyet] Lần " + attempt + ": " + k.errs().size() + " lỗi");
            for (String e : k.errs()) System.out.println("  - " + e);
            StringBuilder fb = new StringBuilder("Cấu hình chưa đạt, hãy sửa và chỉ trả về JSON:");
            for (String e : k.errs()) fb.append("\n- ").append(e);
            messages.add(Json.obj("role", "user", "content", fb.toString()));
        }
        System.out.println("[kiem-duyet] TỪ CHỐI: sau " + soLan + " lần vẫn chưa đạt, không sinh lệnh");
        System.exit(1);
    }
}
