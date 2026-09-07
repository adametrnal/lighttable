#!/usr/bin/env python3
"""Encode timestamped native captures at their original 30 Hz time slots.

Missing capture slots hold the previous image; they never shorten playback.
Requires ffmpeg and ffprobe on PATH. Leaves raw captures intact for verification.
"""
import argparse
import bisect
import json
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('recording', type=Path)
    args = parser.parse_args()
    root = args.recording.resolve(strict=True)
    report = json.loads((root/'recording.json').read_text())
    fps = report['target_fps']
    seconds = report['duration_seconds']
    frames = report['frames']
    slots = [f['slot'] for f in frames]
    count = round(fps * seconds)
    if fps != 30 or seconds != 10 or not slots or slots != sorted(set(slots)):
        raise ValueError('Expected an ordered ten-second, 30 Hz recording')
    if slots[0] != 0 or slots[-1] >= count:
        raise ValueError('Recording must include its first frame and stay inside its duration')
    for frame in frames:
        if int(frame['elapsed_seconds'] * fps) != frame['slot']:
            raise ValueError('Frame timestamp does not match its real-time capture slot')
        if not (root/'captures'/f"{frame['slot']:04}.ppm").is_file():
            raise ValueError('Missing captured image')
    sequence = None
    if 'loupe' in frames[0]:
        timeline_frames = [frames[bisect.bisect_right(slots, slot)-1] for slot in range(count)]
        loupe_slots = [slot for slot, frame in enumerate(timeline_frames) if frame['loupe']]
        cycles = report.get('cycles')
        expected_loupe = ([slot for cycle in range(5) for slot in range(cycle*60+30, cycle*60+60)]
                          if cycles == 5 else list(range(126, 156)))
        if loupe_slots != expected_loupe:
            raise ValueError('Rendered view timing does not match the expected Loupe seconds')
        offsets = [frame['grid_offset'] for frame in timeline_frames]
        if any(b < a for a, b in zip(offsets, offsets[1:])):
            raise ValueError('Grid scrolled upward during the demo')
        if abs(offsets[0]-report['start_offset']) > 1 or abs(offsets[-1]-report['end_offset']) > 1:
            raise ValueError('Grid did not travel from its requested start to the end')
        sequence = {'downward_only': True, 'reached_end': True,
                    'loupe_frames': len(loupe_slots), 'loupe_seconds': len(loupe_slots)/fps,
                    'scrolled_rows': (offsets[-1]-offsets[0])/report['grid_row_pitch']}
        if cycles == 5:
            photo_ids = []
            for cycle in range(5):
                first = cycle*60
                if offsets[first+29] <= offsets[first]:
                    raise ValueError('Expected rapid downward scrolling in each cycle')
                if abs(offsets[first+29]-report['stops'][cycle]['offset']) > 1:
                    raise ValueError('A scrolling cycle did not reach its stop')
                held = timeline_frames[first+30:first+60]
                ids = {frame['photo_id'] for frame in held}
                if len(ids) != 1 or None in ids or not all(frame['loupe_ready'] for frame in held):
                    raise ValueError('Each Loupe second must show one fully decoded photo')
                photo_ids.extend(ids)
            if len(set(photo_ids)) != 5:
                raise ValueError('Expected five different Loupe photos')
            warm = report['warmup_views']
            down = [v['offset'] for v in warm if not v['returning']]
            up = [v['offset'] for v in warm if v['returning']]
            if (not down or not up or any(a > b for a, b in zip(down, down[1:]))
                    or any(a < b for a, b in zip(up, up[1:]))
                    or abs(down[0]-report['start_offset']) > 1
                    or abs(down[-1]-report['end_offset']) > 1
                    or abs(up[0]-report['end_offset']) > 1
                    or abs(up[-1]-report['start_offset']) > 1):
                raise ValueError('Warm-up did not travel down to the bottom and back to the top')
            sequence.update({'cycles': 5, 'loupe_seconds_each': 1.0, 'photo_ids': photo_ids,
                             'warmup_down_and_back': True, 'ends_on_decoded_loupe': True,
                             'grid_frames_with_loading': sum(not f['loupe'] and f['grid_loading'] > 0 for f in timeline_frames),
                             'grid_frames_with_unavailable': sum(not f['loupe'] and f['grid_unavailable'] > 0 for f in timeline_frames)})
        elif offsets[125] <= offsets[0] or offsets[-1] <= offsets[156]:
            raise ValueError('Expected downward scrolling both before and after Loupe')
    output = root/'lighttable-scrolling-10s.mp4'
    if output.exists() or (root/'video-verification.json').exists():
        raise FileExistsError('Choose a fresh recording; output already exists')
    with tempfile.TemporaryDirectory(prefix='lighttable-video-') as temp:
        timeline = Path(temp)
        for slot in range(count):
            source_slot = slots[bisect.bisect_right(slots, slot)-1]
            (timeline/f'{slot:04}.ppm').symlink_to(root/'captures'/f'{source_slot:04}.ppm')
        subprocess.run([
            'ffmpeg', '-hide_banner', '-loglevel', 'warning', '-n',
            '-framerate', str(fps), '-i', str(timeline/'%04d.ppm'),
            '-frames:v', str(count), '-vf', 'scale=trunc(iw/2)*2:trunc(ih/2)*2',
            '-c:v', 'libx264', '-preset', 'medium', '-crf', '19',
            '-pix_fmt', 'yuv420p', '-movflags', '+faststart', str(output),
        ], check=True)
    probe = json.loads(subprocess.check_output([
        'ffprobe', '-v', 'error', '-select_streams', 'v:0', '-count_frames',
        '-show_entries', 'stream=width,height,codec_name,pix_fmt,avg_frame_rate,nb_read_frames,duration',
        '-of', 'json', str(output),
    ]))['streams'][0]
    if int(probe['nb_read_frames']) != count or abs(float(probe['duration'])-seconds) > 0.001:
        raise ValueError('Encoded timeline verification failed')
    subprocess.run(['ffmpeg', '-v', 'error', '-i', str(output), '-f', 'null', '-'], check=True)
    verification = {'video': output.name, 'bytes': output.stat().st_size,
                    'stream': probe, 'captured_frames': len(frames),
                    'held_slots': count-len(frames), 'playback_speed': 1.0,
                    'full_decode_passed': True}
    if sequence is not None:
        verification['sequence'] = sequence
    (root/'video-verification.json').write_text(json.dumps(verification, indent=2)+'\n')
    subprocess.run(['ffmpeg', '-v', 'error', '-n', '-i', str(root/'captures'/'0000.ppm'),
                    '-frames:v', '1', '-update', '1', str(root/'poster.png')], check=True)
    print(json.dumps(verification, indent=2))


if __name__ == '__main__':
    main()
