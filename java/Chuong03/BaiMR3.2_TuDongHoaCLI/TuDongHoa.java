import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Map;
import java.util.regex.Pattern;

/**
 * Bài mở rộng MR3.2 – Tự động hóa cấu hình thiết bị qua CLI (kiểu expect/Netmiko, chỉ dùng socket).
 *
 * Bài mở rộng, không có trong giáo trình; vận dụng Chương 3: dữ liệu CLI là luồng byte KHÔNG có
 * khung thông điệp, nên client phải "đọc cho tới khi gặp dấu nhắc" (READ_UNTIL) – một dạng
 * định khung bằng dấu phân cách, giống READ_LINE ở Bài 3.2 nhưng dấu phân cách là biểu thức chính quy.
 * Quy trình:
 *  1. Đăng nhập, vào chế độ đặc quyền (enable), tắt phân trang (terminal length 0).
 *  2. Với mỗi cổng trong tệp trạng thái mong muốn: đọc cấu hình hiện tại, phân tích,
 *     so sánh và CHỈ sinh lệnh cho phần khác biệt (lũy đẳng – idempotent).
 *  3. Áp dụng lệnh (hoặc chỉ in ra với --thu), đọc lại để kiểm chứng, lưu cấu hình.
 * Tài khoản lấy từ biến môi trường DEVICE_USER, DEVICE_PASSWORD (không ghi cứng trong mã).
 *
 * Chạy: java -cp out TuDongHoa <host> <port> <muc_tieu.json> [--thu]
 */
public class TuDongHoa {
    static final Pattern PROMPT = Pattern.compile("[\\w.-]+(\\([\\w-]+\\))?[>#]$");   // R1>  R1#  R1(config)#  R1(config-if)#

    static final class CliError extends Exception {
        CliError(String m) { super(m); }
    }

    /** Cấu hình một cổng, cùng dạng với tệp mong muốn. */
    record Cong(String moTa, String ip, boolean bat) {}

    static final class CliSession implements AutoCloseable {
        final Socket sock = new Socket();
        final InputStream in;
        final int timeoutMs;
        final ByteArrayOutputStream buf = new ByteArrayOutputStream();   // giữ byte thô: không cắt đôi ký tự UTF-8

        CliSession(String host, int port, int timeoutMs) throws IOException {
            this.timeoutMs = timeoutMs;
            sock.connect(new InetSocketAddress(host, port), timeoutMs);
            in = sock.getInputStream();
        }

        /** READ_UNTIL: đọc dồn vào bộ đệm cho tới khi phần cuối khớp mẫu; trả về phần đã đọc. */
        String readUntil(Pattern pattern) throws IOException, CliError {
            long deadline = System.nanoTime() + timeoutMs * 1_000_000L;
            byte[] chunk = new byte[4096];
            while (true) {
                String all = buf.toString(StandardCharsets.UTF_8);
                String text = all.replaceAll(" +$", "");
                if (pattern.matcher(text).find()) {
                    buf.reset();
                    return text;
                }
                if (all.contains("% Authentication failed")) throw new CliError("Đăng nhập thất bại");
                long remaining = (deadline - System.nanoTime()) / 1_000_000L;
                if (remaining <= 0)
                    throw new CliError("Hết thời gian chờ dấu nhắc, đã nhận: '" + all.substring(Math.max(0, all.length() - 60)) + "'");
                sock.setSoTimeout((int) remaining);
                int n;
                try {
                    n = in.read(chunk);
                } catch (SocketTimeoutException e) {
                    continue;                                   // vòng sau sẽ báo hết thời gian
                }
                if (n == -1) throw new CliError("Thiết bị đóng kết nối");
                buf.write(chunk, 0, n);
            }
        }

        void sendLine(String line) throws IOException {
            sock.getOutputStream().write((line + "\n").getBytes(StandardCharsets.UTF_8));
        }

        void login(String user, String password) throws IOException, CliError {
            readUntil(Pattern.compile("Username:$"));
            sendLine(user);
            readUntil(Pattern.compile("Password:$"));
            sendLine(password);
            readUntil(PROMPT);
        }

        /** Gửi một lệnh, chờ dấu nhắc kế tiếp, trả về phần kết quả (bỏ dòng vọng lại và dấu nhắc). */
        List<String> command(String cmd) throws IOException, CliError {
            sendLine(cmd);
            List<String> out = new ArrayList<>(Arrays.asList(readUntil(PROMPT).replace("\r\n", "\n").split("\n", -1)));
            if (!out.isEmpty() && out.get(0).strip().equals(cmd)) out.remove(0);   // bỏ dòng vọng lại
            List<String> body = out.subList(0, Math.max(0, out.size() - 1));       // bỏ dòng dấu nhắc
            for (String line : body)
                if (line.startsWith("%")) throw new CliError("Thiết bị từ chối lệnh '" + cmd + "': " + body.get(0));
            return new ArrayList<>(body);
        }

        @Override public void close() {
            try { sendLine("exit"); } catch (IOException ignored) { }
            try { sock.close(); } catch (IOException ignored) { }
        }
    }

    /** Phân tích khối 'interface ...' thành Cong cùng dạng với tệp mong muốn. */
    static Cong parseInterface(List<String> lines) {
        String moTa = "", ip = "";
        boolean bat = true;
        for (String line : lines) {
            line = line.strip();
            if (line.startsWith("description ")) moTa = line.substring("description ".length());
            else if (line.startsWith("ip address ")) {
                String[] w = line.split("\\s+");
                ip = DiaChi.ipStr(DiaChi.parseIp(w[2])) + "/" + DiaChi.maskToPrefix(w[3]);
            } else if (line.equals("shutdown")) bat = false;
        }
        return new Cong(moTa, ip, bat);
    }

    /** Chỉ sinh lệnh cho phần khác biệt giữa cấu hình hiện tại và mong muốn. */
    static List<String> diffCommands(Cong cur, Cong want) {
        List<String> cmds = new ArrayList<>();
        if (!cur.moTa().equals(want.moTa())) cmds.add(!want.moTa().isEmpty() ? "description " + want.moTa() : "no description");
        if (!cur.ip().equals(want.ip())) {
            String[] im = DiaChi.splitInterface(want.ip());
            cmds.add("ip address " + im[0] + " " + im[1]);
        }
        if (cur.bat() != want.bat()) cmds.add(want.bat() ? "no shutdown" : "shutdown");
        return cmds;
    }

    @SuppressWarnings("unchecked")
    public static void main(String[] args) throws IOException {
        if (args.length < 3) {
            System.err.println("Cách dùng: java -cp out TuDongHoa <host> <port> <muc_tieu.json> [--thu]");
            System.exit(1);
        }
        String host = args[0], path = args[2];
        int port = Integer.parseInt(args[1]);
        boolean dryRun = Arrays.asList(args).subList(3, args.length).contains("--thu");
        Map<String, Object> want = (Map<String, Object>) Json.parse(Files.readString(Path.of(path), StandardCharsets.UTF_8));
        String user = System.getenv().getOrDefault("DEVICE_USER", "admin");
        String password = System.getenv().getOrDefault("DEVICE_PASSWORD", "");
        System.out.println("[tu-dong] Kết nối " + host + ":" + port + ", tài khoản " + user);
        if (dryRun) System.out.println("[tu-dong] Chế độ chạy thử: chỉ in lệnh, không áp dụng");
        int code = 0;
        try (CliSession cli = new CliSession(host, port, 5000)) {
            try {
                cli.login(user, password);
                cli.command("enable");
                cli.command("terminal length 0");             // tắt --More-- để đọc trọn kết quả
                System.out.println("[tu-dong] " + cli.command("show version").get(0));
                int total = 0;
                for (Object o : (List<Object>) want.get("cong")) {
                    Map<String, Object> item = (Map<String, Object>) o;
                    String name = (String) item.get("ten");
                    Cong w = new Cong((String) item.get("mo_ta"), (String) item.get("ip"), (Boolean) item.get("bat"));
                    Cong cur = parseInterface(cli.command("show running-config interface " + name));
                    List<String> cmds = diffCommands(cur, w);
                    if (cmds.isEmpty()) {
                        System.out.println("[tu-dong] " + name + ": đã đúng, không cần thay đổi");
                        continue;
                    }
                    System.out.println("[tu-dong] " + name + ": cần " + cmds.size() + " lệnh");
                    for (String c : cmds) System.out.println("           + " + c);
                    if (dryRun) continue;
                    cli.command("configure terminal");
                    cli.command("interface " + name);
                    for (String c : cmds) cli.command(c);
                    cli.command("end");
                    Cong after = parseInterface(cli.command("show running-config interface " + name));
                    if (!after.equals(w))                     // đọc lại để kiểm chứng
                        throw new CliError(name + ": cấu hình sau khi áp dụng chưa khớp: " + after);
                    total += cmds.size();
                }
                if (total > 0) {
                    cli.command("write memory");
                    System.out.println("[tu-dong] Đã áp dụng và kiểm chứng " + total + " lệnh, đã lưu cấu hình");
                } else if (!dryRun) {
                    System.out.println("[tu-dong] Thiết bị đã ở đúng trạng thái mong muốn (lũy đẳng)");
                }
                System.out.println("[tu-dong] show ip interface brief:");
                for (String line : cli.command("show ip interface brief")) System.out.println("  " + line);
            } catch (CliError | IOException e) {
                System.out.println("[tu-dong] LỖI: " + e.getMessage());
                code = 1;
            }
        } catch (IOException e) {
            System.out.println("[tu-dong] LỖI: " + e.getMessage());   // không kết nối được thiết bị
            code = 1;
        }
        System.exit(code);
    }
}
