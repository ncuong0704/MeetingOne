import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  createDirectorySpeaker,
  directoryFilterQuery,
  foldVietnamese,
  formatDirectorySpeakerLabel,
  suggestDirectorySpeakers,
  type DirectorySpeaker,
} from './speakerDirectory.ts';

const srcTauriResources = join(
  dirname(fileURLToPath(import.meta.url)),
  '..',
  '..',
  'src-tauri',
  'resources',
);

test('foldVietnamese strips diacritics and đ', () => {
  assert.equal(foldVietnamese('Nguyễn'), 'nguyen');
  assert.equal(foldVietnamese(' Đặng '), 'dang');
  assert.equal(foldVietnamese('PHÒNG KT'), 'phong kt');
});

test('createDirectorySpeaker requires a non-empty full name', () => {
  assert.equal(createDirectorySpeaker({ fullName: '  ' }), null);
  const person = createDirectorySpeaker({
    fullName: '  Nguyễn Văn A  ',
    title: ' Trưởng phòng ',
    department: 'Kế hoạch',
    id: 'fixed-id',
  });
  assert.deepEqual(person, {
    id: 'fixed-id',
    fullName: 'Nguyễn Văn A',
    title: 'Trưởng phòng',
    department: 'Kế hoạch',
  });
});

const people: DirectorySpeaker[] = [
  {
    id: '1',
    fullName: 'Nguyễn Văn A',
    title: 'Trưởng phòng',
    department: 'Kế hoạch',
  },
  {
    id: '2',
    fullName: 'Trần Thị B',
    title: 'Chuyên viên',
    department: 'Tài chính',
  },
  {
    id: '3',
    fullName: 'Nguyễn Văn A',
    title: 'Nhân viên',
    department: 'IT',
  },
];

test('suggestDirectorySpeakers matches folded name, title, or department', () => {
  assert.equal(suggestDirectorySpeakers('nguyen', people).map((p) => p.id).join(','), '1,3');
  assert.equal(suggestDirectorySpeakers('ke hoach', people)[0]?.id, '1');
  assert.equal(suggestDirectorySpeakers('chuyen vien', people)[0]?.id, '2');
});

test('empty query returns people up to the limit, keeping duplicates', () => {
  const all = suggestDirectorySpeakers('', people, 8);
  assert.equal(all.length, 3);
  assert.equal(suggestDirectorySpeakers('xyz', people).length, 0);
});

test('directoryFilterQuery shows full list when the original label is not in the directory', () => {
  assert.equal(directoryFilterQuery('SPEAKER_00', 'SPEAKER_00', people), '');
  assert.equal(directoryFilterQuery('Người nói 1', 'Người nói 1', people), '');
});

test('directoryFilterQuery keeps typed text once the user edits away from the original label', () => {
  assert.equal(directoryFilterQuery('nguyen', 'SPEAKER_00', people), 'nguyen');
  assert.equal(directoryFilterQuery('Nguyễn Văn A', 'Nguyễn Văn A', people), 'Nguyễn Văn A');
});

test('formatDirectorySpeakerLabel joins name, title, and department with dashes', () => {
  assert.equal(
    formatDirectorySpeakerLabel({
      id: 'x',
      fullName: 'Nguyễn Cường',
      title: 'Chuyên viên AI',
      department: 'Phòng CNTT',
    }),
    'Nguyễn Cường - Chuyên viên AI - Phòng CNTT',
  );
});

test('bundled nguoi-noi.json maps Vietnamese keys and skips blank templates', () => {
  const raw = JSON.parse(
    readFileSync(
      join(srcTauriResources, 'mac-dinh', 'nguoi-noi.json'),
      'utf8',
    ),
  ) as Array<{ hoTen?: string; chucVu?: string; phongBan?: string }>;
  const people = raw
    .map((row) =>
      createDirectorySpeaker({
        fullName: row.hoTen ?? '',
        title: row.chucVu,
        department: row.phongBan,
      }),
    )
    .filter((person): person is DirectorySpeaker => person !== null);
  assert.equal(people.length, 5);
  assert.deepEqual(
    people.map((p) => ({ fullName: p.fullName, title: p.title, department: p.department })),
    [
      { fullName: 'Phạm Tuấn Anh', title: 'Tổng Giám đốc', department: 'Ban Điều hành' },
      { fullName: 'Trần Dũng', title: 'Giám đốc Khối Dịch vụ Kỹ thuật', department: 'Khối Dịch vụ kỹ thuật' },
      { fullName: 'Đặng Trần Hùng', title: 'Giám đốc Khối Giải pháp Công nghệ', department: 'Khối Giải pháp - Công nghệ' },
      { fullName: 'Võ Ngọc Sâm', title: 'Giám đốc Khối Dự Án', department: 'Khối Dự án' },
      { fullName: 'Phạm Văn Kiên', title: 'Trưởng phòng KHCT', department: 'Phòng Kế hoạch - Chính trị' },
    ],
  );
});

test('formatDirectorySpeakerLabel skips empty title or department', () => {
  assert.equal(
    formatDirectorySpeakerLabel({
      id: 'x',
      fullName: 'Nguyễn Cường',
      title: '',
      department: 'Phòng CNTT',
    }),
    'Nguyễn Cường - Phòng CNTT',
  );
  assert.equal(
    formatDirectorySpeakerLabel({
      id: 'x',
      fullName: 'Nguyễn Cường',
      title: 'Chuyên viên AI',
      department: '',
    }),
    'Nguyễn Cường - Chuyên viên AI',
  );
  assert.equal(
    formatDirectorySpeakerLabel({
      id: 'x',
      fullName: 'Nguyễn Cường',
      title: '',
      department: '',
    }),
    'Nguyễn Cường',
  );
});
