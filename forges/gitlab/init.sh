#!/bin/bash
set -e

gitlab-rails runner - <<EOS
if User.find_by_username('root').personal_access_tokens.length == 0
  p("creating jj")
  token = User.find_by_username('root').personal_access_tokens.create(
    scopes: [:api, :admin_mode, :read_user, :read_repository, :write_repository, :sudo],
    name: "jj",
    expires_at: PersonalAccessToken::MAX_PERSONAL_ACCESS_TOKEN_LIFETIME_IN_DAYS.days.from_now,
  )
  token.set_token('secret')
  token.save!
  p("jj created and set to expire on #{token.expires_at}")
else
  p("jj already created")
end

# TODO:
#  docker compose exec -it gitlab gitlab-rails console
#   set up root's profile (first name, last name, email, organization)
#   create group
#   create project
#o = Organizations::Organization.find_by_id(1)
#u = User.find_by_username('root')
#u.update(
#  first_name: 'Admin',
#  last_name: 'McAdminFace',
#  email: 'admin@example.com',
#  organization: o,
#)
#u.save!
#
#g = Group.new(
#  name: 'TestGroup',
#  path: 'testgroup',
#)
#g.save!
#
#p = Project.new(
#  name: 'TestProject',
#  path: 'testproject',
#  group: g,
#)
#p.save!
EOS

echo "Post Reconfigure Script successfully executed"
