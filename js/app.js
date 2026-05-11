// Thêm sự kiện click vào nút tạo cuộc họp
document.querySelector('.hero button').addEventListener('click', () => {
    // Mở trang tạo cuộc họp
    window.location.href = 'create-meeting.html';
});

// Thêm sự kiện click vào nút tham gia cuộc họp
document.querySelector('.hero button').addEventListener('click', () => {
    // Mở trang tham gia cuộc họp
    window.location.href = 'join-meeting.html';
});
