def sorted_dependencies(root_dir, excludes = [])
  pattern = root_dir.join('crates', '*', 'Cargo.toml').to_s
  errors = Dir.glob(pattern).sort.flat_map do |path|
    relative = Pathname.new(path).relative_path_from(root_dir).to_s
    next [] if excludes.include?(relative)

    sections = parse_dep_sections(File.read(path))

    %w[dependencies dev-dependencies].filter_map do |section|
      deps = sections[section]
      next if deps.nil? || deps.empty?

      expected = sort_dep_names(deps)
      next if deps == expected

      { path: relative, section: section, actual: deps, expected: expected }
    end
  end

  return true if errors.empty?

  puts 'Found unsorted `[dependencies]` / `[dev-dependencies]` in Cargo.toml.'
  puts 'Entries must be alphabetical, with `shirabe-*` crates listed before others:'
  errors.each do |err|
    puts "  #{err[:path]} [#{err[:section]}]"
    puts "    actual:   #{err[:actual].join(', ')}"
    puts "    expected: #{err[:expected].join(', ')}"
  end
  false
end

def parse_dep_sections(content)
  sections = {}
  current = nil

  content.each_line do |line|
    stripped = line.chomp
    if stripped =~ /\A\s*\[([^\]]+)\]\s*\z/
      current = $1
      sections[current] ||= []
    elsif current && stripped =~ /\A([A-Za-z0-9_-]+)\s*[.=]/
      sections[current] << $1
    end
  end

  sections
end

def sort_dep_names(deps)
  shirabe, other = deps.partition { |d| d.start_with?('shirabe-') }
  shirabe.sort + other.sort
end
