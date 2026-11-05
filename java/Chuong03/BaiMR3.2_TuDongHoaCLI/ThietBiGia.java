import java.io.BufferedInputStream;
import java.io.ByteArrayOutputStream;
import java.io.EOFException;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Bài mở rộng MR3.2 – Thiết bị mạng mô phỏng có giao diện dòng lệnh (CLI) qua TCP.
 *
 * Mô phỏng một bộ định tuyến kiểu Telnet để luyện tự động hóa mà không cần thiết bị thật:
 *  - Đăng nhập: "Username: " / "Password: " (mật khẩu lấy từ biến môi trường DEVICE_PASSWORD).
 *  - Dấu nhắc theo chế độ: R1>  R1#  R1(config)#  R1(config-if)#
 *  - Lệnh: enable, terminal length 0, show version, show ip interface brief,
 *    show running-config [interface <tên>], configure terminal, interface <tên>,
 *    description <chữ>, ip address <ip> <mặt_nạ>, [no] shutdown, exit, end, write memory.
 *  - Mỗi dòng lệnh được "vọng lại" (echo) như terminal thật; dòng kết thúc bằng CRLF.
 *  - Khi chưa tắt phân trang, "show running-config" dừng sau 8 dòng với " --More-- ".
 * Trạng thái cấu hình giữ trong bộ nhớ, dùng chung cho mọi phiên (có khóa).
 * CẢNH BÁO: giống Telnet, mật khẩu đi dạng rõ; thiết bị thật phải dùng SSH.
 *
 * Chạy: java -cp out ThietBiGia [host] [port]      (mặc định 127.0.0.1 9000)
 */
public class ThietBiGia {
    static final String HOSTNAME = "R1";
    static final String USER = System.getenv().getOrDefault("DEVICE_USER", "admin");
    static final String PASSWORD = System.getenv().getOrDefault("DEVICE_PASSWORD", "lab123");
    static final int PAGE = 8;                                  // số dòng mỗi trang khi bật phân trang

    /** Trạng thái một cổng. */
    static final class Iface {
        String desc, ip, mask;
        boolean shut;
        Iface(String desc, String ip, String mask, boolean shut) { this.desc = desc; this.ip = ip; this.mask = mask; this.shut = shut; }
    }

    static final Object stateLock = new Object();
    static final Map<String, Iface> interfaces = new LinkedHashMap<>();   // trạng thái ban đầu của thiết bị
    static {
        interfaces.put("Gi0/0", new Iface("Lien ket R2", "10.20.1.225", "255.255.255.252", false));
        interfaces.put("Gi0/1", new Iface("", "", "", true));
        interfaces.put("Gi0/2", new Iface("", "", "", true));
    }
    static final AtomicInteger sessions = new AtomicInteger();

    static void log(String text) { System.out.println("[thiet-bi] " + text); }

    /** Sinh cấu hình đang chạy từ trạng thái (toàn bộ hoặc một cổng). */
    static List<String> runningConfig(String only) {
        List<String> lines = new ArrayList<>();
        if (only == null) lines.addAll(List.of("!", "hostname " + HOSTNAME, "!"));
        synchronized (stateLock) {
            for (Map.Entry<String, Iface> e : interfaces.entrySet()) {
                if (only != null && !e.getKey().equals(only)) continue;
                Iface i = e.getValue();
                lines.add("interface " + e.getKey());
                if (!i.desc.isEmpty()) lines.add(" description " + i.desc);
                lines.add(!i.ip.isEmpty() ? " ip address " + i.ip + " " + i.mask : " no ip address");
                if (i.shut) lines.add(" shutdown");
                lines.add("!");
            }
        }
        if (only == null) lines.add("end");
        return lines;
    }

    static List<String> ipBrief() {
        List<String> lines = new ArrayList<>(List.of(String.format("%-10s%-14s%-22s%s", "Interface", "IP-Address", "Status", "Protocol")));
        synchronized (stateLock) {
            for (Map.Entry<String, Iface> e : interfaces.entrySet()) {
                Iface i = e.getValue();
                String status = i.shut ? "administratively down" : "up";
                lines.add(String.format("%-10s%-14s%-22s%s", e.getKey(), i.ip.isEmpty() ? "unassigned" : i.ip, status,
                        i.shut ? "down" : "up"));
            }
        }
        return lines;
    }

    static final class Session {
        final Socket conn;
        final int sid;
        final InputStream in;
        final OutputStream out;
        String mode = "user", iface = null;
        boolean paging = true;

        Session(Socket conn, int sid) throws IOException {
            this.conn = conn;
            this.sid = sid;
            this.in = new BufferedInputStream(conn.getInputStream());   // đọc theo dòng và theo từng byte
            this.out = conn.getOutputStream();
        }

        void send(String text) throws IOException { out.write(text.getBytes(StandardCharsets.UTF_8)); }

        String readline() throws IOException {
            ByteArrayOutputStream b = new ByteArrayOutputStream();
            int c;
            while ((c = in.read()) != -1) {
                b.write(c);
                if (c == '\n') break;
            }
            if (b.size() == 0) throw new EOFException();
            return b.toString(StandardCharsets.UTF_8).replaceAll("[\r\n]+$", "");
        }

        void prompt() throws IOException {
            String suffix = switch (mode) {
                case "user" -> ">";
                case "enable" -> "#";
                case "config" -> "(config)#";
                default -> "(config-if)#";
            };
            send(HOSTNAME + suffix);
        }

        /** In nhiều dòng; nếu đang phân trang thì dừng mỗi PAGE dòng chờ một phím bất kỳ. */
        void show(List<String> lines) throws IOException {
            for (int k = 1; k <= lines.size(); k++) {
                send(lines.get(k - 1) + "\r\n");
                if (paging && k % PAGE == 0 && k < lines.size()) {
                    send(" --More-- ");
                    if (in.read() == -1) throw new EOFException();
                    send("\r          \r");                    // xóa chữ --More-- như terminal thật
                }
            }
        }

        boolean login() throws IOException {
            send("\r\n" + HOSTNAME + " - thiết bị mô phỏng cho bài thực hành\r\n\r\nUsername: ");
            String user = readline();
            send("Password: ");
            String pw = readline();                             // không vọng lại mật khẩu
            send("\r\n");
            if (!user.equals(USER) || !pw.equals(PASSWORD)) {
                log("phiên #" + sid + ": đăng nhập SAI (user='" + user + "')");
                send("% Authentication failed\r\n");
                return false;
            }
            log("phiên #" + sid + ": đăng nhập " + user);
            return true;
        }

        void run() throws IOException {
            if (!login()) return;
            while (true) {
                prompt();
                String cmd = readline().strip();
                send(cmd + "\r\n");                             // vọng lại dòng lệnh
                if (cmd.isEmpty()) continue;
                if (!execute(cmd)) break;
            }
        }

        void invalid() throws IOException { send("% Invalid input detected at '^' marker.\r\n"); }

        List<String> version() {
            return List.of("Thiết bị mô phỏng R1, phần mềm SIM-IOS 1.0", interfaces.size() + " cổng GigabitEthernet");
        }

        boolean execute(String cmd) throws IOException {
            String[] words = cmd.split("\\s+");
            String m = mode;
            if (cmd.equals("exit") && (m.equals("user") || m.equals("enable"))) return false;
            if (m.equals("user")) {
                if (cmd.equals("enable")) mode = "enable";
                else if (cmd.equals("show version")) show(version());
                else invalid();
            } else if (m.equals("enable")) {
                if (cmd.equals("terminal length 0")) paging = false;
                else if (cmd.equals("show version")) show(version());
                else if (cmd.equals("show ip interface brief")) show(ipBrief());
                else if (cmd.equals("show running-config")) show(runningConfig(null));
                else if (words.length == 4 && Arrays.asList(words).subList(0, 3).equals(List.of("show", "running-config", "interface"))) {
                    if (interfaces.containsKey(words[3])) show(runningConfig(words[3]));
                    else invalid();
                } else if (cmd.equals("configure terminal") || cmd.equals("conf t")) {
                    send("Enter configuration commands, one per line.  End with CNTL/Z.\r\n");
                    mode = "config";
                } else if (cmd.equals("write memory") || cmd.equals("wr")) {
                    send("Building configuration...\r\n[OK]\r\n");
                    log("phiên #" + sid + ": lưu cấu hình (write memory)");
                } else invalid();
            } else if (cmd.equals("end")) {
                mode = "enable";
                iface = null;
            } else if (m.equals("config")) {
                if (words[0].equals("interface") && words.length == 2 && interfaces.containsKey(words[1])) {
                    mode = "config-if";
                    iface = words[1];
                } else if (cmd.equals("exit")) mode = "enable";
                else invalid();
            } else if (m.equals("config-if")) {
                configIf(cmd, words);
            }
            return true;
        }

        void configIf(String cmd, String[] words) throws IOException {
            if (cmd.equals("exit")) {
                mode = "config";
                iface = null;
                return;
            }
            synchronized (stateLock) {
                Iface i = interfaces.get(iface);
                if (words[0].equals("description") && words.length > 1) i.desc = cmd.substring("description ".length());
                else if (cmd.equals("no description")) i.desc = "";
                else if (words.length == 4 && words[0].equals("ip") && words[1].equals("address")) {
                    try {
                        DiaChi.parseIp(words[2]);
                        if (!words[3].matches("\\d{1,2}") || Integer.parseInt(words[3]) > 32)
                            DiaChi.maskToPrefix(words[3]);          // mặt nạ dạng chấm hoặc độ dài tiền tố
                    } catch (IllegalArgumentException e) {
                        send("% Invalid address\r\n");
                        return;
                    }
                    i.ip = words[2];
                    i.mask = words[3];
                } else if (cmd.equals("shutdown")) i.shut = true;
                else if (cmd.equals("no shutdown")) i.shut = false;
                else {
                    invalid();
                    return;
                }
            }
            log("phiên #" + sid + ": " + iface + ": " + cmd);
        }
    }

    static void handle(Socket conn) {
        int sid = sessions.incrementAndGet();
        String addr = conn.getInetAddress().getHostAddress() + ":" + conn.getPort();
        try (conn) {
            new Session(conn, sid).run();
        } catch (IOException e) {                               // EOFException cũng là IOException
            // client đóng kết nối hoặc lỗi mạng: kết thúc phiên
        } finally {
            log("phiên #" + sid + " (" + addr + ") kết thúc");
        }
    }

    public static void main(String[] args) throws IOException {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int port = args.length > 1 ? Integer.parseInt(args[1]) : 9000;
        Runtime.getRuntime().addShutdownHook(new Thread(() -> log("Dừng thiết bị")));   // Ctrl+C
        try (ServerSocket srv = new ServerSocket()) {
            srv.setReuseAddress(true);
            srv.bind(new InetSocketAddress(host, port));
            log(HOSTNAME + " lắng nghe CLI tại " + host + ":" + port);
            while (true) {
                Socket conn = srv.accept();
                Thread t = new Thread(() -> handle(conn));
                t.setDaemon(true);
                t.start();
            }
        }
    }
}
