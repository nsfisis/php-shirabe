def no_std_collections_maps(root_dir, excludes = [])
  pattern = root_dir.join('crates', '**', '*.rs').to_s
  errors = Dir.glob(pattern).sort.flat_map do |path|
    relative = Pathname.new(path).relative_path_from(root_dir).to_s
    next [] if excludes.include?(relative)

    find_std_map_usages(path, relative)
  end

  return true if errors.empty?

  puts 'Found uses of `std::collections::{HashMap, HashSet, BTreeMap, BTreeSet}`.'
  puts 'Use `indexmap::IndexMap` / `indexmap::IndexSet` instead:'
  errors.each do |err|
    puts "  #{err}"
  end
  false
end

BANNED_MAP_NAMES = %w[HashMap HashSet BTreeMap BTreeSet].freeze

def find_std_map_usages(path, relative)
  errors = []

  File.readlines(path).each_with_index do |raw, idx|
    code = raw.split('//', 2).first || raw

    code.scan(/\bstd::collections::(HashMap|HashSet|BTreeMap|BTreeSet)\b/) do |m|
      errors << "#{relative}:#{idx + 1}: use of `std::collections::#{m[0]}` (use `indexmap::#{indexmap_replacement(m[0])}` instead)"
    end

    code.scan(/\bstd::collections::\{([^}]*)\}/) do |m|
      m[0].split(',').each do |entry|
        name = entry.strip.split(/\s+as\s+/).first
        next unless BANNED_MAP_NAMES.include?(name)

        errors << "#{relative}:#{idx + 1}: import of `std::collections::#{name}` (use `indexmap::#{indexmap_replacement(name)}` instead)"
      end
    end
  end

  errors.uniq
end

def indexmap_replacement(name)
  case name
  when 'HashMap', 'BTreeMap' then 'IndexMap'
  when 'HashSet', 'BTreeSet' then 'IndexSet'
  end
end
