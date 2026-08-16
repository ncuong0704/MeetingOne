'use client';

import { useCallback, useEffect, useState } from 'react';
import { Pencil, Plus, Trash2 } from 'lucide-react';
import { toast } from 'sonner';
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
      <div className="max-w-xl space-y-6 animate-pulse">
        <div className="h-28 bg-paper-3 rounded-md" />
        <div className="h-40 bg-paper-3 rounded-md" />
      </div>
    );
  }

  return (
    <div className="max-w-xl space-y-6">
      <section>
        <h2 className="text-sm font-semibold text-ink tracking-tight">
          {editingId ? 'Sửa người nói' : 'Thêm người nói'}
        </h2>
        <p className="text-xs text-ink-2 mt-0.5 mb-2">
          Gợi ý khi gán tên Người nói 1-9.
        </p>
        <div className="app-surface overflow-hidden px-4 py-3 space-y-3">
          <div className="space-y-1.5">
            <Label htmlFor="speaker-full-name" className="text-sm font-medium text-ink">
              Họ tên
            </Label>
            <Input
              id="speaker-full-name"
              value={form.fullName}
              onChange={(event) => setForm((prev) => ({ ...prev, fullName: event.target.value }))}
              placeholder="Nguyễn Văn A"
              className="h-9"
            />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1.5">
              <Label htmlFor="speaker-title" className="text-sm font-medium text-ink">
                Chức vụ
              </Label>
              <Input
                id="speaker-title"
                value={form.title}
                onChange={(event) => setForm((prev) => ({ ...prev, title: event.target.value }))}
                placeholder="Trưởng phòng"
                className="h-9"
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="speaker-department" className="text-sm font-medium text-ink">
                Phòng ban
              </Label>
              <Input
                id="speaker-department"
                value={form.department}
                onChange={(event) => setForm((prev) => ({ ...prev, department: event.target.value }))}
                placeholder="Kế hoạch"
                className="h-9"
              />
            </div>
          </div>
          <div className="flex gap-1.5 pt-0.5">
            <button
              type="button"
              onClick={() => void handleSubmit()}
              disabled={saving}
              className="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-2.5 text-xs font-medium text-primary-foreground hover:bg-primary-hover disabled:opacity-50"
            >
              <Plus className="h-3.5 w-3.5" />
              {editingId ? 'Lưu' : 'Thêm'}
            </button>
            {editingId && (
              <button
                type="button"
                onClick={resetForm}
                disabled={saving}
                className="inline-flex h-8 items-center gap-1.5 rounded-md border border-rule bg-paper-2 px-2.5 text-xs font-medium text-ink-2 hover:bg-secondary hover:text-ink disabled:opacity-50"
              >
                Hủy
              </button>
            )}
          </div>
        </div>
      </section>

      <section>
        <h2 className="text-sm font-semibold text-ink tracking-tight mb-2">
          Danh sách ({people.length})
        </h2>
        <div className="app-surface overflow-hidden">
          {people.length === 0 ? (
            <p className="px-4 py-3 text-xs text-ink-2">
              Chưa có người nói. Thêm họ tên ở form trên.
            </p>
          ) : (
            <ul className="divide-y divide-rule">
              {people.map((person) => {
                const meta = [person.title, person.department].filter(Boolean).join(', ');
                return (
                  <li key={person.id} className="px-4 py-3 flex items-center justify-between gap-3">
                    <div className="min-w-0">
                      <p className="text-sm font-medium text-ink truncate">{person.fullName}</p>
                      {meta ? (
                        <p className="text-xs text-ink-2 mt-0.5 truncate">{meta}</p>
                      ) : null}
                    </div>
                    <div className="flex shrink-0 gap-0.5">
                      <button
                        type="button"
                        title="Sửa"
                        onClick={() => handleEdit(person)}
                        className="inline-flex h-8 w-8 items-center justify-center rounded-md text-ink-2 hover:bg-secondary hover:text-ink"
                      >
                        <Pencil className="h-3.5 w-3.5" />
                      </button>
                      <button
                        type="button"
                        title="Xóa"
                        onClick={() => void handleDelete(person.id)}
                        className="inline-flex h-8 w-8 items-center justify-center rounded-md text-ink-2 hover:bg-destructive/10 hover:text-destructive"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </section>
    </div>
  );
}
