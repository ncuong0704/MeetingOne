'use client';

import { useCallback, useEffect, useState } from 'react';
import { Pencil, Plus, Trash2, Users } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  createDirectorySpeaker,
  getSpeakerDirectory,
  saveSpeakerDirectory,
  type DirectorySpeaker,
} from '@/lib/speakerDirectory';

const emptyForm = { fullName: '', title: '', department: '' };

export function SpeakerDirectorySettings() {
  const [people, setPeople] = useState<DirectorySpeaker[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState(emptyForm);

  const persist = useCallback(async (next: DirectorySpeaker[]) => {
    setSaving(true);
    try {
      const saved = await saveSpeakerDirectory(next);
      setPeople(saved);
      return saved;
    } catch (error) {
      toast.error('Không lưu được danh sách người nói');
      throw error;
    } finally {
      setSaving(false);
    }
  }, []);

  useEffect(() => {
    getSpeakerDirectory()
      .then(setPeople)
      .catch(() => toast.error('Không tải được danh sách người nói'))
      .finally(() => setLoading(false));
  }, []);

  const resetForm = () => {
    setForm(emptyForm);
    setEditingId(null);
  };

  const handleSubmit = async () => {
    const person = createDirectorySpeaker({
      ...form,
      id: editingId ?? undefined,
    });
    if (!person) {
      toast.error('Nhập họ tên người nói');
      return;
    }
    const without = people.filter((item) => item.id !== person.id);
    await persist([...without, person]);
    toast.success(editingId ? 'Đã cập nhật người nói' : 'Đã thêm người nói');
    resetForm();
  };

  const handleEdit = (person: DirectorySpeaker) => {
    setEditingId(person.id);
    setForm({
      fullName: person.fullName,
      title: person.title,
      department: person.department,
    });
  };

  const handleDelete = async (id: string) => {
    await persist(people.filter((person) => person.id !== id));
    if (editingId === id) resetForm();
  };

  if (loading) {
    return (
      <div className="animate-pulse space-y-3">
        <div className="h-24 bg-gray-200 rounded-xl" />
        <div className="h-40 bg-gray-200 rounded-xl" />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
        <div className="px-5 py-4 border-b border-gray-50">
          <h3 className="text-base font-semibold text-gray-900">
            {editingId ? 'Sửa người nói' : 'Thêm người nói'}
          </h3>
          <p className="text-sm text-gray-500 mt-1">
            Danh sách dùng để gợi ý khi gán tên trong «Người nói 1–9».
          </p>
        </div>
        <div className="px-5 py-4 grid gap-3 sm:grid-cols-3">
          <div className="space-y-1.5 sm:col-span-1">
            <Label htmlFor="speaker-full-name">Họ tên</Label>
            <Input
              id="speaker-full-name"
              value={form.fullName}
              onChange={(event) => setForm((prev) => ({ ...prev, fullName: event.target.value }))}
              placeholder="Nguyễn Văn A"
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="speaker-title">Chức vụ</Label>
            <Input
              id="speaker-title"
              value={form.title}
              onChange={(event) => setForm((prev) => ({ ...prev, title: event.target.value }))}
              placeholder="Trưởng phòng"
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="speaker-department">Phòng ban</Label>
            <Input
              id="speaker-department"
              value={form.department}
              onChange={(event) => setForm((prev) => ({ ...prev, department: event.target.value }))}
              placeholder="Kế hoạch"
            />
          </div>
        </div>
        <div className="px-5 pb-4 flex gap-2">
          <Button variant="blue" onClick={() => void handleSubmit()} disabled={saving}>
            <Plus className="w-4 h-4" />
            {editingId ? 'Lưu thay đổi' : 'Thêm'}
          </Button>
          {editingId && (
            <Button variant="outline" onClick={resetForm} disabled={saving}>
              Hủy
            </Button>
          )}
        </div>
      </div>

      <div className="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
        <div className="px-5 py-4 border-b border-gray-50 flex items-center gap-2">
          <Users className="w-4 h-4 text-gray-500" />
          <h3 className="text-base font-semibold text-gray-900">
            Danh sách ({people.length})
          </h3>
        </div>
        {people.length === 0 ? (
          <p className="px-5 py-8 text-sm text-gray-500 text-center">
            Chưa có người nói. Thêm họ tên ở form trên.
          </p>
        ) : (
          <ul className="divide-y divide-gray-50">
            {people.map((person) => {
              const meta = [person.title, person.department].filter(Boolean).join(' · ');
              return (
                <li key={person.id} className="px-5 py-3 flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="text-sm font-medium text-gray-900 truncate">{person.fullName}</p>
                    {meta ? (
                      <p className="text-xs text-gray-500 mt-0.5 truncate">{meta}</p>
                    ) : null}
                  </div>
                  <div className="flex shrink-0 gap-1">
                    <button
                      type="button"
                      title="Sửa"
                      onClick={() => handleEdit(person)}
                      className="p-1.5 rounded-md text-gray-500 hover:bg-gray-100 hover:text-gray-800"
                    >
                      <Pencil className="w-4 h-4" />
                    </button>
                    <button
                      type="button"
                      title="Xóa"
                      onClick={() => void handleDelete(person.id)}
                      className="p-1.5 rounded-md text-gray-500 hover:bg-red-50 hover:text-red-600"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </div>
  );
}
