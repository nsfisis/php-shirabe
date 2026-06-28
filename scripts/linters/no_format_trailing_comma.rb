def no_format_trailing_comma(root_dir, excludes = [])
  crates_dir = root_dir.join('crates')
  raw = `grep -rn --include='*.rs' -e ',)' #{crates_dir}`

  errors = []
  raw.each_line do |line|
    line = line.chomp
    path, lineno, content = line.split(':', 3)
    next if content.nil?

    relative = Pathname.new(path).relative_path_from(root_dir).to_s
    next if excludes.include?(relative)

    # `(,)` is a macro repetition fragment (e.g. `$(,)?`).
    next if content.match?(/\(,\)/)

    errors << "#{relative}:#{lineno}: trailing `,)` before a closing paren"
  end

  return true if errors.empty?

  puts 'Found `,)` introduced by formatting. Remove it.'
  errors.each do |err|
    puts "  #{err}"
  end
  false
end
