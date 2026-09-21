# Hướng Dẫn Xử Lý Khi Phần Mềm Báo: "Toolchain Chưa Sẵn Sàng"

> **Dành cho**: Giáo viên, Học sinh và Phụ huynh  
> **Tóm tắt nhanh**: Đây **không phải lỗi hỏng mạch hay đứt dây**, bạn chỉ cần **chờ 1 – 3 phút** để ứng dụng tải xong bộ nạp chương trình trong lần đầu sử dụng.

---

## 1. "Toolchain Chưa Sẵn Sàng" Là Gì?

- **"Toolchain" là gì?**  
  Bạn có thể hiểu đơn giản đây là **"Bộ nạp chương trình"** – công cụ giúp dịch các khối lệnh Scratch mà học sinh lập trình thành ngôn ngữ để mạch robot (ESP32, Arduino) có thể hiểu và chạy được.
  
- **Tại sao lại có thông báo chờ tải?**  
  Để file cài đặt ban đầu của ứng dụng Windify Link rất nhẹ (tải về chỉ mất vài giây), bộ nạp chương trình này sẽ được ứng dụng **tự động tải ngầm về máy tính trong lần đầu tiên bạn mở phần mềm**.

> [!NOTE]
> 💡 **Khẳng định quan trọng:**  
> Mạch robot, dây cáp và máy tính của bạn hoàn toàn bình thường. Bạn chỉ cần giữ mạng Internet ổn định và đợi ứng dụng tải xong là có thể sử dụng bình thường. Quá trình này **chỉ diễn ra duy nhất 1 lần đầu tiên**.

---

## 2. Dấu Hiệu Nhận Biết

Khi bấm nút **"Tải lên" (Upload)** trên màn hình lập trình Scratch:
1. Màn hình hiện thông báo đỏ (Upload failed).
2. Có dòng chữ tiếng Anh:  
   `Toolchain is not ready yet — still downloading. Check /status for progress.`  
   *(Nghĩa là: Bộ nạp chương trình chưa sẵn sàng — vẫn đang tải về. Vui lòng chờ...)*
3. Ở góc dưới bên phải màn hình máy tính (gần đồng hồ) có thể xuất hiện thông báo nhỏ: *"Downloading toolchain..."*.

---

## 3. Các Bước Xử Lý Đơn Giản

> 📋 **Tóm tắt quy trình xử lý nhanh 4 bước:**
> 
> 1. **Thấy thông báo:** *"Toolchain is not ready yet"* ➔ Không rút dây cáp, không tắt phần mềm.
> 2. **Kiểm tra tiến trình:** Mở trình duyệt vào `http://localhost:11337/status` xem `%` tải tại mục `setupProgress`.
> 3. **Chờ 1 – 3 phút:** Đợi ứng dụng tải đủ 100% (`ready: true`).
> 4. **Bấm nạp lại:** Quay lại Scratch, bấm nút **"Tải lên"** màu xanh để bắt đầu nạp code vào robot.


### Bước 1: Giữ nguyên ứng dụng và kết nối Internet
- **Không rút dây cáp** nối mạch với máy tính.
- **Không tắt ứng dụng Windify Link** đang chạy dưới thanh công cụ máy tính.
- Đảm bảo máy tính đang kết nối Wi-Fi hoặc mạng dây ổn định.

### Bước 2: Xem ứng dụng đã tải được bao nhiêu %
Mở một tab mới trên trình duyệt web và nhập địa chỉ:
```text
http://localhost:11337/status
```
Màn hình sẽ hiển thị thông tin dạng:
- Dòng `"setupProgress": 65` nghĩa là **đã tải được 65%**.
- Dòng `"ready": false` nghĩa là đang tải, chưa xong.

### Bước 3: Chờ 1 – 3 phút
- Bạn chỉ cần đợi từ 1 đến 3 phút (tùy theo tốc độ mạng Internet).
- Khi tải xong, trang web trên sẽ tự đổi thành:
  - `"ready": true` (Đã sẵn sàng!)
  - `"setupProgress": 100` (Đã tải xong 100%)
- Góc phải màn hình máy tính sẽ hiện thông báo: *"Toolchain ready"*.

### Bước 4: Bấm nạp lại chương trình
1. Quay lại trang lập trình Scratch.
2. Bấm nút **Đóng** trên bảng thông báo lỗi cũ.
3. Bấm lại nút **"Tải lên" (Upload)** màu xanh.
4. Chương trình sẽ nạp vào robot ngay lập tức!

*(Từ các lần học tiếp theo, phần mềm sẽ nạp ngay mà không cần phải chờ đợi nữa).*

---

## 4. Dành Riêng Cho Thầy/Cô Quản Lý Phòng Tin Học (Cài Đặt Nhanh Không Cần Mạng)

Nếu phòng máy của trường **không có mạng Internet** hoặc mạng yếu, thầy/cô có thể cài đặt sẵn bộ nạp cho toàn bộ máy học sinh bằng USB theo các bước sau:

1. **Chuẩn bị trên USB**:
   - Sao chép thư mục `tools` đã có sẵn từ máy giáo viên (nằm tại đường dẫn: `C:\futureacademy\tools`).
2. **Chép sang máy học sinh**:
   - Cắm USB vào máy học sinh và dán thư mục `tools` vào đúng vị trí:
     ```text
     C:\futureacademy\tools
     ```
3. **Kết quả**:
   - Khi học sinh mở ứng dụng Windify Link lên, phần mềm sẽ tự động nhận diện bộ công cụ có sẵn và **sẵn sàng sử dụng ngay 100%**, không cần kết nối mạng Internet.

---

## 5. Các Câu Hỏi Thường Gặp (FAQ)

### ❓ Lần sau mở máy lên tôi có phải chờ tải lại không?
**Trả lời:** **Không**. Quá trình tải này chỉ diễn ra duy nhất một lần đầu tiên sau khi cài đặt. Những lần sau, bạn chỉ cần cắm robot và bấm nạp là chạy ngay.

### ❓ Nếu đang tải mà bị mất mạng Internet thì sao?
**Trả lời:** Không sao cả. Khi có mạng trở lại, ứng dụng sẽ tự động tải tiếp phần còn lại từ điểm bị ngắt, bạn không cần phải cài lại từ đầu.

### ❓ Tôi có cần khởi động lại máy tính không?
**Trả lời:** **Không cần**. Khi tải xong (đạt 100%), bạn chỉ cần bấm nút nạp trên Scratch là xong.
