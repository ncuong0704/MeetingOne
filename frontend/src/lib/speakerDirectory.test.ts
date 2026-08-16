import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  createDirectorySpeaker,
  directoryFilterQuery,
  foldVietnamese,
  formatDirectorySpeakerLabel,
  suggestDirectorySpeakers,
  type DirectorySpeaker,
} from './speakerDirectory.ts';

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
