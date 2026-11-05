import java.awt.Color;
import java.awt.Component;
import java.awt.Dimension;
import java.awt.GridBagConstraints;
import java.awt.GridBagLayout;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.function.Consumer;
import javax.swing.Box;
import javax.swing.JButton;
import javax.swing.JCheckBox;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JProgressBar;
import javax.swing.JScrollPane;
import javax.swing.JTable;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;
import javax.swing.table.DefaultTableCellRenderer;
import javax.swing.table.DefaultTableModel;

/**
 * Bài mở rộng MR6.13 – Bảng điều khiển máy chủ bán vé có phòng chờ ảo (Swing).
 *
 * Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
 * ở GiaoDienChung.CuaSoServerCoSo; tệp này vẽ khung "Tài nguyên": vé còn lại (số lớn + thanh), số người đang chờ, sổ
 * đã bán (đỏ khi vượt), số đang phục vụ / đã phát, lượt đã cấp / hết hạn, yêu cầu mua đang xử lý / giới hạn và số lần
 * từ chối quá tải, ô "Cấp lượt … người/s" + "Áp dụng ngay" (đổi tốc độ khi đang chạy), mức token trong thùng, tham số
 * đặt lại (tổng vé, thùng token, hạn lượt, giới hạn xử lý, ô chọn bật phòng chờ) và nút "So sánh bật/tắt phòng chờ"
 * với bảng hai dòng (tắt / bật).
 *
 * Chạy:  java -cp out ServerGui [host] [cổng]        (mặc định 127.0.0.1 43772)
 */
public class ServerGui extends GiaoDienChung.CuaSoServerCoSo {
    JLabel nhanVe, nhanHang, nhanBan, nhanPhucVu, nhanXuLy, nhanToken;
    JProgressBar thanhVe;
    JTextField oTocDo;
    JButton nutTocDo, nutSoSanhPc;
    final Map<String, JTextField> oTs = new java.util.LinkedHashMap<>();
    JCheckBox oPc;
    DefaultTableModel bangPc;
    JTable bangPcT;
    List<Map<String, Object>> hangPc = new ArrayList<>();

    ServerGui(String host, int cong) {
        super("Máy chủ bán vé MR6.13", "Tài nguyên: vé và phòng chờ ảo",
                bao -> new MayChu.MayChuPhongCho("khoa", 2.0, bao, Json.obj()), host, cong, 1000, 200, MoPhong.SO_LAN_MAC_DINH);
        xay();
    }

    MayChu.MayChuPhongCho mc() { return (MayChu.MayChuPhongCho) lop; }

    @Override
    protected void veTaiNguyen(JPanel g) {
        JPanel f = new JPanel(new GridBagLayout());
        f.setBackground(GiaoDienChung.NEN);
        f.add(GiaoDienChung.nhan("VÉ CÒN LẠI", GiaoDienChung.chu(11, true), new Color(0x404040)), GiaoDienChung.o(0, 0));
        f.add(GiaoDienChung.nhan("      HÀNG ĐỢI", GiaoDienChung.chu(11, true), new Color(0x404040)), GiaoDienChung.o(1, 0));
        nhanVe = GiaoDienChung.nhan("–", GiaoDienChung.chu(20, true), new Color(0x8b0000));
        nhanVe.setPreferredSize(new Dimension(130, 28));
        nhanHang = GiaoDienChung.nhan("–", GiaoDienChung.chu(20, true), new Color(0x1f4fa0));
        nhanHang.setPreferredSize(new Dimension(160, 28));
        f.add(nhanVe, GiaoDienChung.o(0, 1));
        GridBagConstraints c = GiaoDienChung.o(1, 1);
        c.insets = new java.awt.Insets(1, 30, 1, 2);
        f.add(nhanHang, c);
        g.add(GiaoDienChung.hang(f));
        thanhVe = new JProgressBar(0, 500);
        thanhVe.setPreferredSize(new Dimension(360, 14));
        thanhVe.setMaximumSize(new Dimension(360, 14));
        thanhVe.setAlignmentX(0);
        g.add(thanhVe);
        nhanBan = GiaoDienChung.nhan(" ", GiaoDienChung.chu(12, false), null);
        nhanPhucVu = GiaoDienChung.nhan(" ", GiaoDienChung.chu(12, false), null);
        nhanXuLy = GiaoDienChung.nhan(" ", GiaoDienChung.chu(12, true), null);
        g.add(nhanBan);
        g.add(nhanPhucVu);
        g.add(nhanXuLy);
        oTocDo = GiaoDienChung.oNhap(Chung.g(Chung.sod(lop.thamSo.get("toc_do"))), 6);
        nutTocDo = GiaoDienChung.nut("Áp dụng ngay", GiaoDienChung.XANH_DUONG, 0);
        nutTocDo.addActionListener(e -> doiTocDo());
        nhanToken = GiaoDienChung.nhan(" ", GiaoDienChung.chu(11, false), null);
        g.add(GiaoDienChung.hang(GiaoDienChung.nhan("Cấp lượt:"), oTocDo, GiaoDienChung.nhan("người/s"), nutTocDo, nhanToken));
        f = new JPanel(new GridBagLayout());
        f.setBackground(GiaoDienChung.NEN);
        Object[][] o = {{"tong_ve", "Tổng vé", 6}, {"dung_luong", "Thùng token", 5}, {"luot_s", "Hạn lượt (s)", 5},
                {"gioi_han_xu_ly", "Giới hạn xử lý", 5}};
        for (int i = 0; i < o.length; i++) {
            String ma = (String) o[i][0];
            Object v = lop.thamSo.get(ma);
            f.add(GiaoDienChung.nhan((i % 2 == 1 ? "  " : "") + o[i][1] + ":"), GiaoDienChung.o((i % 2) * 2, i / 2));
            JTextField x = GiaoDienChung.oNhap(v instanceof Double d ? Chung.g(d) : v, (int) o[i][2]);
            oTs.put(ma, x);
            f.add(x, GiaoDienChung.o((i % 2) * 2 + 1, i / 2));
        }
        oPc = new JCheckBox("Bật phòng chờ ảo (có hiệu lực khi Đặt lại)", Chung.so(lop.thamSo.get("phong_cho")) != 0);
        oPc.setBackground(GiaoDienChung.NEN);
        c = GiaoDienChung.o(0, 2);
        c.gridwidth = 4;
        f.add(oPc, c);
        g.add(GiaoDienChung.hang(f));
        nutSoSanhPc = GiaoDienChung.nut("So sánh bật/tắt phòng chờ", GiaoDienChung.CAM, 210);
        nutSoSanhPc.addActionListener(e -> soSanhPc());
        g.add(GiaoDienChung.hang(nutSoSanhPc));
        bangPc = new DefaultTableModel(new Object[] {"Phòng chờ", "Mua được", "Hết vé", "Quá tải", "Bỏ cuộc", "p99 ms"}, 0) {
            @Override public boolean isCellEditable(int r, int cc) { return false; }
        };
        bangPcT = new JTable(bangPc);
        bangPcT.setRowHeight(18);
        bangPcT.setFocusable(false);
        bangPcT.setRowSelectionAllowed(false);
        bangPcT.getTableHeader().setReorderingAllowed(false);
        DefaultTableCellRenderer ve = new DefaultTableCellRenderer() {
            @Override
            public Component getTableCellRendererComponent(JTable t, Object v, boolean chon, boolean td, int r, int cc) {
                super.getTableCellRendererComponent(t, v, false, false, r, cc);
                boolean do_ = r < hangPc.size() && Chung.so(hangPc.get(r).get("qua_tai")) > 0;
                setBackground(do_ ? new Color(0xffc8c8) : new Color(0xd8f5d8));
                setForeground(do_ ? GiaoDienChung.DO_DAM : Color.BLACK);
                setHorizontalAlignment(cc == 0 ? LEFT : RIGHT);
                return this;
            }
        };
        int[] rong = {74, 66, 52, 58, 60, 58};
        for (int i = 0; i < rong.length; i++) {
            bangPcT.getColumnModel().getColumn(i).setPreferredWidth(rong[i]);
            bangPcT.getColumnModel().getColumn(i).setCellRenderer(ve);
        }
        bangPcT.setPreferredScrollableViewportSize(new Dimension(368, 2 * 18));
        JScrollPane cuon = new JScrollPane(bangPcT);
        cuon.setAlignmentX(0);
        cuon.setMaximumSize(new Dimension(372, 62));
        cuon.getViewport().setBackground(Color.WHITE);
        g.add(Box.createVerticalStrut(2));
        g.add(cuon);
    }

    @Override
    protected Map<String, Object> thamSoDatLai() {
        try {
            return Json.obj("tong_ve", Long.parseLong(oTs.get("tong_ve").getText().strip()),
                    "dung_luong", Long.parseLong(oTs.get("dung_luong").getText().strip()),
                    "luot_s", Double.parseDouble(oTs.get("luot_s").getText().strip()),
                    "gioi_han_xu_ly", Long.parseLong(oTs.get("gioi_han_xu_ly").getText().strip()),
                    "toc_do", Double.parseDouble(oTocDo.getText().strip()), "phong_cho", oPc.isSelected() ? 1L : 0L);
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException("tham số phải là số");
        }
    }

    /** Điền ô tham số / ô chọn (kịch bản demo) – như người dùng gõ. */
    void datThamSo(Map<String, Object> ts) {
        ts.forEach((k, v) -> {
            if (k.equals("phong_cho")) {
                if (oPc.isSelected() != (Chung.so(v) != 0)) oPc.doClick(0);
            } else (k.equals("toc_do") ? oTocDo : oTs.get(k)).setText(String.valueOf(v));
        });
    }

    void doiTocDo() {
        try {
            mc().datTocDo(Double.parseDouble(oTocDo.getText().strip()));
        } catch (IllegalArgumentException e) {
            nhatKy.ghi("Tốc độ không hợp lệ: " + e.getMessage());
        }
    }

    // ------------------------------------------------------------ so sánh bật/tắt phòng chờ (luồng nền)
    void soSanhPc() {
        int n, k;
        try {
            n = Integer.parseInt(oN.getText().strip());
            k = Integer.parseInt(oK.getText().strip());
            if (n < 1 || k < 1) throw new NumberFormatException();
        } catch (NumberFormatException e) {
            nhatKy.ghi("n, k, lần ≥ 1");
            return;
        }
        if (dangMoPhong || !lop.chay) {
            nhatKy.ghi("Cần START SERVER và chờ lượt mô phỏng trước xong");
            return;
        }
        dangMoPhong = true;
        nutBan.setEnabled(false);
        nutSoSanh.setEnabled(false);
        nutSoSanhPc.setEnabled(false);
        nhanMoPhong.setText("đang chạy…");
        nhanMoPhong.setForeground(GiaoDienChung.DO_DAM);
        String dc = lop.diaChi;
        int i = dc.lastIndexOf(':');
        String h = dc.substring(0, i);
        int cg = Integer.parseInt(dc.substring(i + 1));
        int kk = Math.min(k, n);
        Consumer<String> bao = text -> SwingUtilities.invokeLater(() -> nhatKy.ghi("[mô phỏng] " + text));
        Chung.chayNen("mo_phong", () -> {
            List<Map<String, Object>> kq;
            try {
                kq = MoPhong.soSanhPhongCho(h, cg, n, kk, bao, "khoa");
            } catch (Exception e) {
                bao.accept("lỗi: " + e.getMessage());
                kq = List.of();
            }
            List<Map<String, Object>> k2 = kq;
            SwingUtilities.invokeLater(() -> xongSoSanhPc(k2));
        });
    }

    private void xongSoSanhPc(List<Map<String, Object>> hang) {
        dangMoPhong = false;
        nutBan.setEnabled(true);
        nutSoSanh.setEnabled(true);
        nutSoSanhPc.setEnabled(true);
        nhanMoPhong.setText("xong");
        nhanMoPhong.setForeground(GiaoDienChung.XANH_DAM);
        if (hang.isEmpty()) return;
        hangPc = new ArrayList<>(hang);
        bangPc.setRowCount(0);
        for (Map<String, Object> x : hang)
            bangPc.addRow(new Object[] {x.get("phong_cho"), Chung.py(x.get("mua_duoc")), Chung.py(x.get("het_ve")),
                    Chung.py(x.get("qua_tai")), Chung.py(x.get("bo_cuoc")), MoPhong.ms1((Double) x.get("p99_ms"))});
        bangPcT.repaint();
        for (String d : MoPhong.bangPhongCho(hang)) nhatKy.ghi(d.strip());
    }

    // ------------------------------------------------------------ vẽ lại định kỳ
    @Override
    protected void capNhatTaiNguyen(Map<String, Object> tt, Map<String, Object> tk) {
        Map<String, Object> sk = Chung.map(tt.get("su_kien"));
        long tong = Chung.so(sk.get("tong_ve")), con = Chung.so(tt.get("con_lai")), banSo = Chung.so(tt.get("ban_so"));
        boolean pc = Chung.dung(tt.get("phong_cho"));
        nhanVe.setText(con + " / " + tong);
        thanhVe.setMaximum((int) tong);
        thanhVe.setValue((int) Math.max(0, con));
        nhanHang.setText(pc ? tt.get("dang_cho") + " người" : "TẮT");
        nhanHang.setForeground(pc ? new Color(0x1f4fa0) : new Color(0x808080));
        boolean vuot = banSo > tong;
        nhanBan.setText("Sổ đã bán: " + banSo + " vé, " + tt.get("so_don") + " đơn" + (vuot ? " – VƯỢT " + (banSo - tong) + " vé!" : "")
                + (Chung.dung(tt.get("het_ve")) ? "  [HẾT VÉ]" : ""));
        nhanBan.setForeground(vuot ? GiaoDienChung.DO_DAM : Color.BLACK);
        nhanPhucVu.setText("Đang phục vụ tới #" + tt.get("dang_phuc_vu") + " / đã phát #" + tt.get("so_cap") + " · lượt đã cấp "
                + tt.get("da_cap_luot") + ", hết hạn " + tt.get("het_han"));
        long qt = Chung.so(tt.get("qua_tai"));
        nhanXuLy.setText("Mua đang xử lý: " + tt.get("dang_xu_ly") + " / " + tt.get("gioi_han") + " (max " + tt.get("xu_ly_max")
                + ") · từ chối quá tải: " + qt);
        nhanXuLy.setForeground(qt > 0 ? GiaoDienChung.DO_DAM : GiaoDienChung.XANH_DAM);
        nhanToken.setText("thùng " + tt.get("token") + "/" + tt.get("dung_luong") + " token");
        if (nutSoSanhPc.isEnabled() == dangMoPhong) nutSoSanhPc.setEnabled(!dangMoPhong);   // khóa khi đang mô phỏng
    }

    public static void main(String[] args) {
        GiaoDienChung.NhatKy.inRa = true;                            // chạy riêng: nhật ký cũng ra dòng lệnh
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43772;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new ServerGui(host, cong).frame.setVisible(true);
        });
    }
}
