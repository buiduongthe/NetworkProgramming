import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.SocketAddress;
import java.net.SocketTimeoutException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Bài mở rộng MR4.2 – Server đua xe thời gian thực trên UDP (server có thẩm quyền, vòng tick 20 Hz).
 *
 * Mô hình:
 *  - Client gửi ĐẦU VÀO (ga/nhả/phanh) kèm số thứ tự = tick mà đầu vào áp dụng; mỗi gói mang kèm 3 đầu vào
 *    gần nhất nên mất một gói vẫn còn bản sao ở gói sau.
 *  - Server chạy vòng lặp tick cố định 50 ms: lấy đầu vào của tick hiện tại cho từng xe; nếu không có
 *    (mọi bản sao đều mất) thì GIỮ đầu vào cũ; mô phỏng vật lý; gửi ẢNH CHỤP trạng thái tới mọi client.
 *  - Ảnh chụp mang số tick: client chỉ giữ ảnh mới nhất, bỏ ảnh đến muộn (cũ hơn).
 *  - --mat, --dao giả lập mất gói và đảo thứ tự (theo hàm băm có hạt giống nên tái lập được).
 *    Danh sách "0,20" nghĩa là cuộc đua 1 mất 0 %, cuộc đua 2 mất 20 %.
 *
 * Chạy: java -cp out ServerDuaXe [host] [port] [--so-xe 2] [--mat 0] [--dao 0] [--seed 1] [--so-van 0]
 */
public class ServerDuaXe {
    static final int DIR_IN = 0, DIR_SNAP = 1, DIR_REORDER = 2;

    static final class Car {
        final String name;
        final SocketAddress addr;
        int id = -1, x = 0, v = 0;
        int done = -1;                              // số tick từ lúc xuất phát đến khi về đích (-1: chưa)
        final Map<Integer, Integer> inputs = new HashMap<>();   // tick -> đầu vào đã nhận (bộ đệm đầu vào)
        int lastIn = 0;                             // đầu vào áp dụng gần nhất (dùng lại khi thiếu)
        int recv, dropped, held, crashes;
        int snapSent, snapDropped, snapReordered;
        byte[] delayed = null;                      // ảnh chụp đang bị giữ lại để gửi muộn (giả lập đảo thứ tự)
        Car(String name, SocketAddress addr) { this.name = name; this.addr = addr; }
    }

    static List<Integer> pctList(String text) {
        List<Integer> r = new ArrayList<>();
        for (String p : text.split(",")) r.add(Integer.parseInt(p.strip()));
        return r;
    }

    final DatagramSocket sock;
    final int soXe;
    final long seed;
    final List<Integer> mat, dao;
    int race = 0;
    Map<SocketAddress, Car> cars;
    String state;                                   // CHO -> DUA (gồm đếm ngược) -> XONG
    int tick;
    int endTick;
    Long nextTick;                                  // thời điểm tick kế tiếp (System.nanoTime), null khi chờ xe

    ServerDuaXe(DatagramSocket sock, int soXe, String mat, String dao, long seed) {
        this.sock = sock; this.soXe = soXe; this.seed = seed;
        this.mat = pctList(mat); this.dao = pctList(dao);
        reset();
    }

    void reset() {
        cars = new LinkedHashMap<>();
        state = "CHO";
        tick = -1;
        endTick = -1;
        nextTick = null;
    }

    void sendRaw(SocketAddress addr, byte[] data) {
        try {
            sock.send(new DatagramPacket(data, data.length, addr));     // SENDTO
        } catch (IOException ignored) { }
    }

    void send(SocketAddress addr, Map<String, Object> obj) {
        sendRaw(addr, Json.stringify(obj).getBytes(StandardCharsets.UTF_8));
    }

    // ----- nhận gói -----
    @SuppressWarnings("unchecked")
    void onPacket(byte[] data, SocketAddress addr) {
        Object o;
        try {
            o = Json.parse(new String(data, StandardCharsets.UTF_8));
        } catch (RuntimeException e) {
            return;                                 // datagram hỏng: bỏ qua
        }
        if (!(o instanceof Map<?, ?>)) return;
        Map<String, Object> p = (Map<String, Object>) o;
        if ("join".equals(p.get("t"))) onJoin(p, addr);
        else if ("in".equals(p.get("t"))) onInput(p, addr);
    }

    void onJoin(Map<String, Object> p, SocketAddress addr) {
        if (!(p.get("name") instanceof String name) || name.codePointCount(0, name.length()) < 1
                || name.codePointCount(0, name.length()) > 12) return;
        Car car = cars.get(addr);
        if (car == null || state.equals("XONG")) {
            if (!state.equals("CHO") || cars.values().stream().anyMatch(c -> c.name.equals(name))) {
                send(addr, Json.obj("t", "busy"));  // đang đua hoặc trùng tên: client thử lại sau
                return;
            }
            car = new Car(name, addr);
            cars.put(addr, car);
            System.out.println("[server] Thêm một xe từ " + ((InetSocketAddress) addr).getAddress().getHostAddress()
                    + " (" + cars.size() + "/" + soXe + ")");
        }
        // trả lời cả khi client gửi lại join (welcome trước đó có thể đã mất)
        send(addr, Json.obj("t", "welcome", "name", car.name, "need", soXe, "race", race + 1, "mat", loss(), "dao", reorder()));
        if (state.equals("CHO") && cars.size() == soXe) startRace();
    }

    void onInput(Map<String, Object> p, SocketAddress addr) {
        Car car = cars.get(addr);
        if (car == null || !state.equals("DUA") || !(p.get("seq") instanceof Long seqL) || !(p.get("in") instanceof List<?> inputs))
            return;
        int seq = seqL.intValue();
        if (DuaXe.lossy(seed, DIR_IN, car.id, seq, loss())) {
            car.dropped++;                          // giả lập: gói này "mất trên đường"
            return;
        }
        car.recv++;
        for (int j = 0; j < Math.min(3, inputs.size()); j++) {   // inputs[j] là đầu vào của tick seq - j
            int t = seq - j;
            if (t > tick && inputs.get(j) instanceof Long inp && inp >= -1 && inp <= 1)   // chỉ nhận cho tick chưa mô phỏng
                car.inputs.putIfAbsent(t, inp.intValue());
        }
    }

    // ----- cuộc đua -----
    int loss() { return mat.get(Math.min(race, mat.size() - 1)); }
    int reorder() { return dao.get(Math.min(race, dao.size() - 1)); }

    List<Car> ordered() {
        List<Car> l = new ArrayList<>(cars.values());
        l.sort(Comparator.comparingInt(c -> c.id));
        return l;
    }

    void startRace() {
        List<Car> byName = new ArrayList<>(cars.values());
        byName.sort(Comparator.comparing(c -> c.name));
        for (int i = 0; i < byName.size(); i++) byName.get(i).id = i;   // mã xe theo thứ tự tên
        state = "DUA";
        tick = -1;
        nextTick = System.nanoTime() + DuaXe.TICK_MS * 1_000_000L;
        List<String> names = ordered().stream().map(c -> c.name).toList();
        System.out.println("[server] Cuộc đua " + (race + 1) + ": " + String.join(", ", names) + "; mất gói " + loss()
                + " %, đảo thứ tự " + reorder() + " %; xuất phát sau " + DuaXe.START + " tick");
    }

    void doTick() {
        tick++;
        int k = tick;
        boolean racing = state.equals("DUA") && k >= DuaXe.START;
        for (Car car : ordered()) {
            Integer inp = car.inputs.remove(k);
            car.inputs.keySet().removeIf(t -> t < k);   // dọn đầu vào quá hạn
            if (inp == null) {
                inp = car.lastIn;                       // thiếu đầu vào: giữ đầu vào cũ
                if (racing && car.done < 0) car.held++;
            }
            car.lastIn = inp;
            if (!racing || car.done >= 0) continue;
            DuaXe.Step s = DuaXe.step(car.x, car.v, inp);
            car.x = s.x();
            car.v = s.v();
            if (s.crashed()) {
                car.crashes++;
                System.out.println("[server] t=" + (k - DuaXe.START) + ": " + car.name + " văng ở khúc cua (quá "
                        + DuaXe.fmt2(DuaXe.V_CUA) + " ô/tick)");
            }
            if (car.x >= DuaXe.TRACK) {
                car.x = DuaXe.TRACK;
                car.v = 0;
                car.done = k - DuaXe.START + 1;
            }
        }
        if (state.equals("DUA") && (cars.values().stream().allMatch(c -> c.done >= 0) || k >= DuaXe.START + DuaXe.MAX_RACE)) {
            state = "XONG";
            endTick = k;
            List<String> rank = new ArrayList<>();
            List<Car> r = ranking();
            for (int i = 0; i < r.size(); i++)
                rank.add((i + 1) + ". " + r.get(i).name + " " + (r.get(i).done >= 0 ? String.valueOf(r.get(i).done) : "-") + " tick");
            System.out.println("[server] Cuộc đua " + (race + 1) + " kết thúc: " + String.join("; ", rank));
        }
        broadcast(k);
        if (state.equals("XONG") && k >= endTick + DuaXe.END_TICKS) finishRace();
    }

    List<Car> ranking() {
        List<Car> l = new ArrayList<>(cars.values());
        l.sort(Comparator.comparing((Car c) -> c.done < 0).thenComparingInt(c -> c.done)
                .thenComparingInt(c -> -c.x).thenComparingInt(c -> c.id));
        return l;
    }

    void broadcast(int k) {
        List<Object> names = new ArrayList<>(), carList = new ArrayList<>();
        for (Car c : ordered()) {
            names.add(c.name);
            carList.add(List.of(c.id, c.x, c.v, c.done));
        }
        Map<String, Object> snap = Json.obj("t", "snap", "race", race + 1, "tick", k, "phase", state, "names", names, "cars", carList);
        if (state.equals("XONG")) {
            List<Object> rank = new ArrayList<>();
            for (Car c : ranking()) rank.add(List.of(c.name, c.done));
            snap.put("rank", rank);
        }
        byte[] data = Json.stringify(snap).getBytes(StandardCharsets.UTF_8);
        for (Car car : ordered()) {
            byte[] prev = car.delayed;
            car.delayed = null;
            if (DuaXe.lossy(seed, DIR_SNAP, car.id, k, loss())) {
                car.snapDropped++;
            } else if (DuaXe.lossy(seed, DIR_REORDER, car.id, k, reorder())) {
                car.delayed = data;                     // giữ lại, gửi SAU ảnh chụp tick kế tiếp
                car.snapReordered++;
            } else {
                sendRaw(car.addr, data);
                car.snapSent++;
            }
            if (prev != null) {
                sendRaw(car.addr, prev);                // ảnh chụp cũ đến sau ảnh mới: client phải bỏ
                car.snapSent++;
            }
        }
    }

    void finishRace() {
        for (Car c : ordered()) {
            int total = c.recv + c.dropped;
            System.out.println("[server] " + c.name + ": nhận " + c.recv + "/" + total + " gói đầu vào (giả lập mất " + c.dropped
                    + "), " + c.held + " tick phải giữ đầu vào cũ, văng " + c.crashes + " lần");
        }
        race++;
        reset();
    }

    static volatile boolean normalExit = false;

    public static void main(String[] args) throws Exception {
        List<String> pos = new ArrayList<>();
        int soXe = 2, soVan = 0;
        String mat = "0", dao = "0";
        long seed = 1;
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--so-xe" -> soXe = Integer.parseInt(args[++i]);
                case "--mat" -> mat = args[++i];
                case "--dao" -> dao = args[++i];
                case "--seed" -> seed = Long.parseLong(args[++i]);
                case "--so-van" -> soVan = Integer.parseInt(args[++i]);
                default -> pos.add(args[i]);
            }
        }
        String host = pos.size() > 0 ? pos.get(0) : "0.0.0.0";
        int port = pos.size() > 1 ? Integer.parseInt(pos.get(1)) : 9200;
        DatagramSocket sock = new DatagramSocket(new InetSocketAddress(host, port));
        System.out.println("[server] Đua xe UDP tại " + host + ":" + port + ", " + (1000 / DuaXe.TICK_MS) + " tick/s, cần "
                + soXe + " xe, đường dài " + (DuaXe.TRACK / 100) + " ô");
        Runtime.getRuntime().addShutdownHook(new Thread(() -> { if (!normalExit) System.out.println("[server] Dừng server"); }));
        ServerDuaXe srv = new ServerDuaXe(sock, soXe, mat, dao, seed);
        byte[] buf = new byte[2048];
        while (true) {
            // Chờ gói tới, tối đa tới thời điểm tick kế tiếp (thay cho select có thời gian chờ của bản Python).
            boolean wait = true;
            if (srv.nextTick == null) sock.setSoTimeout(0);              // 0 = chờ mãi
            else {
                long ms = (srv.nextTick - System.nanoTime() + 999_999) / 1_000_000;
                if (ms <= 0) wait = false;
                else sock.setSoTimeout((int) ms);
            }
            if (wait) {
                DatagramPacket pk = new DatagramPacket(buf, buf.length);
                try {
                    sock.receive(pk);
                    srv.onPacket(java.util.Arrays.copyOf(pk.getData(), pk.getLength()), pk.getSocketAddress());
                } catch (SocketTimeoutException e) {
                    // tới giờ tick
                } catch (IOException e) {
                    // Windows: ICMP "port unreachable" từ client đã thoát – bỏ qua
                }
            }
            if (srv.nextTick != null && System.nanoTime() >= srv.nextTick) {
                srv.nextTick += DuaXe.TICK_MS * 1_000_000L;               // lịch cố định: không trôi theo thời gian xử lý
                srv.doTick();
                if (srv.state.equals("CHO") && soVan > 0 && srv.race >= soVan) {
                    System.out.println("[server] Đã đua " + srv.race + " cuộc, dừng server");
                    break;
                }
            }
        }
        normalExit = true;
        sock.close();
    }
}
